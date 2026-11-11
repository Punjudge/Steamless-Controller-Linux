//! The system tray icon and menu (StatusNotifierItem, as KDE Plasma shows
//! it), with the original tray's items.

use crate::config::AutoMode;
use crate::controller::{Command, Shared, Status};
use crate::desktop;
use crate::eventlog;
use ksni::menu::{CheckmarkItem, MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// Icon and tooltip, as TrayApp::UpdateTrayIcon (0x140013230) picks them:
/// sharing with Steam (game mode on while Steam also holds the controller),
/// then on, then no virtual gamepad, then connected, then nothing found.
pub fn tray_look(st: &Status) -> (&'static str, &'static str) {
    let shared = st.game_mode && st.others_holding.iter().any(|h| h.contains("steam"));
    let icon = if st.game_mode { "input-gaming" } else { "input-gaming-symbolic" };
    let tip = if shared {
        "Steamless Mode ON (sharing with Steam)"
    } else if st.game_mode {
        "Steamless Mode ON"
    } else if st.pad_error.is_some() {
        "No virtual gamepad (/dev/uinput)"
    } else if st.connected {
        "Connected (Steamless Mode OFF)"
    } else {
        "No controller found"
    };
    (icon, tip)
}

pub struct Tray {
    shared: Arc<Shared>,
}

fn show_window(shared: &Shared) {
    shared.show_window.store(true, Ordering::SeqCst);
    shared.request_repaint();
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "steamless-controller".into()
    }

    fn title(&self) -> String {
        "Steamless Controller".into()
    }

    fn icon_name(&self) -> String {
        tray_look(&self.shared.status()).0.into()
    }

    /// Always Active. The original's icon never leaves the tray; it only swaps
    /// between the on and off pictures (TrayApp::UpdateTrayIcon, NIM_MODIFY with
    /// NIF_ICON). Passive made KDE Plasma hide the icon while Steamless Mode was
    /// off, leaving no way to reach or quit a hidden window (issue #1).
    fn status(&self) -> ksni::Status {
        ksni::Status::Active
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let (icon, text) = tray_look(&self.shared.status());
        ksni::ToolTip {
            title: "Steamless Controller".into(),
            description: text.into(),
            icon_name: icon.into(),
            icon_pixmap: Vec::new(),
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        show_window(&self.shared);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let st = self.shared.status();
        let (mode, eol, notes) = {
            let s = self.shared.settings.read().unwrap();
            (s.auto_mode, s.enable_on_launch, s.notifications)
        };
        let battery = self.shared.live.battery.load(Ordering::Relaxed);
        let selected = AutoMode::ALL.iter().position(|m| *m == mode).unwrap_or(0);
        let mut items: Vec<MenuItem<Self>> = vec![
            CheckmarkItem {
                label: "Steamless Mode".into(),
                checked: st.want_control,
                activate: Box::new(|t: &mut Self| t.shared.send(Command::ToggleManual)),
                ..Default::default()
            }
            .into(),
            SubMenu {
                label: "Control Mode".into(),
                submenu: vec![RadioGroup {
                    selected,
                    select: Box::new(|t: &mut Self, i: usize| {
                        if let Some(m) = AutoMode::ALL.get(i) {
                            t.shared.send(Command::SetAutoMode(*m));
                        }
                    }),
                    options: AutoMode::ALL
                        .iter()
                        .map(|m| RadioItem { label: m.label().into(), ..Default::default() })
                        .collect(),
                }
                .into()],
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Customize Controls...".into(),
                activate: Box::new(|t: &mut Self| show_window(&t.shared)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: "Start with login".into(),
                checked: desktop::autostart_enabled(),
                activate: Box::new(|_t: &mut Self| {
                    let on = !desktop::autostart_enabled();
                    if let Err(e) = desktop::set_autostart(on) {
                        eventlog::write(format!("AUTOSTART: could not change: {e}"));
                    }
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Enable on Launch".into(),
                checked: eol,
                activate: Box::new(|t: &mut Self| {
                    let mut s = t.shared.settings.write().unwrap();
                    s.enable_on_launch = !s.enable_on_launch;
                    s.save();
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Open Event Log".into(),
                activate: Box::new(|_t: &mut Self| desktop::open_path(&eventlog::path())),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Show Notifications".into(),
                checked: notes,
                activate: Box::new(|t: &mut Self| {
                    let mut s = t.shared.settings.write().unwrap();
                    s.notifications = !s.notifications;
                    s.save();
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
        ];
        if battery >= 0 {
            items.push(
                StandardItem { label: format!("Battery - {battery}%"), enabled: false, ..Default::default() }.into(),
            );
        }
        items.push(
            StandardItem {
                label: "Exit".into(),
                activate: Box::new(|t: &mut Self| {
                    t.shared.quit.store(true, Ordering::SeqCst);
                    t.shared.send(Command::Quit);
                    t.shared.request_repaint();
                }),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

/// Starts the tray icon. Returns false when the desktop has no tray to show it in.
pub fn spawn(shared: Arc<Shared>) -> bool {
    use ksni::blocking::TrayMethods;
    let handle = match (Tray { shared: shared.clone() }).spawn() {
        Ok(h) => h,
        Err(e) => {
            eventlog::write(format!("TRAY: not available: {e}"));
            return false;
        }
    };
    std::thread::Builder::new()
        .name("tray-refresh".into())
        .spawn(move || {
            let mut last = String::new();
            while !shared.quit.load(Ordering::SeqCst) {
                let st = shared.status();
                let s = shared.settings.read().unwrap();
                let now = format!(
                    "{}{}{}{:?}{}{}{}",
                    st.game_mode,
                    st.connected,
                    st.want_control,
                    s.auto_mode,
                    s.enable_on_launch,
                    s.notifications,
                    shared.live.battery.load(Ordering::Relaxed)
                );
                drop(s);
                if now != last {
                    last = now;
                    handle.update(|_| {});
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            handle.shutdown().wait();
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use ksni::Tray as _;

    #[test]
    fn icon_stays_visible_when_steamless_mode_is_off() {
        let tray = Tray { shared: Shared::new(Settings::default()) };
        for on in [false, true] {
            tray.shared.status.lock().unwrap().game_mode = on;
            assert!(matches!(tray.status(), ksni::Status::Active), "game_mode={on}");
        }
    }

    /// The tooltip order of 0x140013230.
    #[test]
    fn tooltip_order_matches_program() {
        let mut st = Status::default();
        assert_eq!(tray_look(&st).1, "No controller found");
        st.connected = true;
        assert_eq!(tray_look(&st).1, "Connected (Steamless Mode OFF)");
        st.pad_error = Some("no access".into());
        assert_eq!(tray_look(&st).1, "No virtual gamepad (/dev/uinput)");
        st.game_mode = true;
        assert_eq!(tray_look(&st), ("input-gaming", "Steamless Mode ON"));
        st.others_holding = vec!["steam (pid 1)".into()];
        assert_eq!(tray_look(&st).1, "Steamless Mode ON (sharing with Steam)");
    }
}
