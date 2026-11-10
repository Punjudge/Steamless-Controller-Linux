//! The system tray icon and menu (StatusNotifierItem, as KDE Plasma shows
//! it), with the original tray's items.

use crate::config::AutoMode;
use crate::controller::{Command, Shared};
use crate::desktop;
use crate::eventlog;
use ksni::menu::{CheckmarkItem, MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

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
        let st = self.shared.status();
        if st.game_mode {
            "input-gaming".into()
        } else if st.connected {
            "input-gaming-symbolic".into()
        } else {
            "input-gaming-symbolic".into()
        }
    }

    fn status(&self) -> ksni::Status {
        if self.shared.status().game_mode { ksni::Status::Active } else { ksni::Status::Passive }
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let st = self.shared.status();
        let text = if st.game_mode {
            "Steamless Mode ON"
        } else if st.connected {
            "Connected (Steamless Mode OFF)"
        } else {
            "No controller found"
        };
        ksni::ToolTip {
            title: "Steamless Controller".into(),
            description: text.into(),
            icon_name: "input-gaming".into(),
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
