//! The controller manager: decides when to take the controller (control
//! modes), opens every game-input interface, turns lizard mode off, keeps it
//! off, and turns each state report into virtual gamepad, keyboard and mouse
//! input. Mirrors ControllerManager / TrayApp in the original.

use crate::config::*;
use crate::eventlog;
use crate::games::{self, SteamState};
use crate::hid::{self, HidDevice, HidInfo};
use crate::mapping::*;
use crate::protocol::{self as p, Transport};
use crate::uinput::{self, FfEvent, PadFrame, PadKind, Touch, UinputDevice, VirtualPad};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const KEEPALIVE: Duration = Duration::from_secs(2);
const STALL: Duration = Duration::from_secs(4);
const POWER_OFF_GRACE: Duration = Duration::from_secs(3);
const RELEASE_GRACE: Duration = Duration::from_secs(5);
const DEVICE_SYNC: Duration = Duration::from_secs(1);
const PROCESS_SCAN: Duration = Duration::from_secs(2);
/// TrayApp heartbeat timer (id 4, SetTimer 900000 ms at 0x14000ccf7).
const HEARTBEAT: Duration = Duration::from_secs(15 * 60);
/// A beat later than this (0xea600 ms at 0x14000bd87) means the loop was blocked.
const HEARTBEAT_SLACK: Duration = Duration::from_secs(60);
/// Less Steam activity must be seen on this many polls in a row before it
/// counts (SteamWatcher.cpp:10 LESS_STEAM_POLL_COUNT, ~6 s at 2 s polls).
const LESS_STEAM_POLL_COUNT: u32 = 3;

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub connected: bool,
    pub transport: Option<Transport>,
    pub game_mode: bool,
    pub live_slots: usize,
    pub want_control: bool,
    pub steam: Option<SteamState>,
    pub active_game: Option<String>,
    pub pad_error: Option<String>,
    pub access_error: Option<String>,
    pub others_holding: Vec<String>,
    pub platform: Option<Platform>,
}

#[derive(Debug)]
pub enum Command {
    ToggleManual,
    SetManual(bool),
    SetAutoMode(AutoMode),
    SettingsChanged,
    Quit,
}

pub struct Live {
    pub trigger: [AtomicI32; 2],
    pub stick: [AtomicI32; 4],
    pub battery: AtomicI32,
}

pub struct Shared {
    pub settings: RwLock<Settings>,
    pub status: Mutex<Status>,
    pub live: Live,
    /// Press-to-bind: set by the window, answered by the read loop.
    pub capturing: AtomicBool,
    pub captured: Mutex<Option<Binding>>,
    /// The profile the window is editing right now (live preview), if any.
    pub preview: Mutex<Option<Profile>>,
    pub commands: Mutex<Vec<Command>>,
    pub repaint: Mutex<Option<Box<dyn Fn() + Send>>>,
    pub show_window: AtomicBool,
    /// The "virtual gamepad unavailable" notice went out (TrayApp's
    /// m_vigemBalloonShown): once per failure, until a pad comes up or the
    /// controller is released.
    pub pad_warned: AtomicBool,
    pub quit: AtomicBool,
    /// Watch only: never take the controller (the screenshot self-test).
    pub passive: AtomicBool,
    profile: RwLock<Arc<Profile>>,
}

impl Shared {
    pub fn new(settings: Settings) -> Arc<Self> {
        let profile = Arc::new(settings.default_profile.clone());
        Arc::new(Self {
            settings: RwLock::new(settings),
            status: Mutex::new(Status::default()),
            live: Live {
                trigger: [AtomicI32::new(0), AtomicI32::new(0)],
                stick: [AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0)],
                battery: AtomicI32::new(-1),
            },
            capturing: AtomicBool::new(false),
            captured: Mutex::new(None),
            preview: Mutex::new(None),
            commands: Mutex::new(Vec::new()),
            repaint: Mutex::new(None),
            show_window: AtomicBool::new(false),
            pad_warned: AtomicBool::new(false),
            quit: AtomicBool::new(false),
            passive: AtomicBool::new(false),
            profile: RwLock::new(profile),
        })
    }

    pub fn send(&self, c: Command) {
        self.commands.lock().unwrap().push(c);
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn request_repaint(&self) {
        if let Some(f) = self.repaint.lock().unwrap().as_ref() {
            f();
        }
    }

    fn current_profile(&self) -> Arc<Profile> {
        self.profile.read().unwrap().clone()
    }
}

/// TrayApp::UpdateTrayIcon's balloon (0x140013230): shown only with
/// notifications on, and only once until the flag is cleared.
fn pad_unavailable_notice(shared: &Shared) {
    if !shared.settings.read().unwrap().notifications {
        return;
    }
    if shared.pad_warned.swap(true, Ordering::SeqCst) {
        return;
    }
    notify(shared, "Virtual gamepad unavailable",
        "The virtual controller could not be created. Your login needs access to /dev/uinput (Steam's device rules or the input group). The event log has the details.");
}

pub fn notify(shared: &Shared, title: &str, body: &str) {
    eventlog::write(format!("NOTIFY: {title} — {body}"));
    if !shared.settings.read().unwrap().notifications {
        return;
    }
    let _ = std::process::Command::new("notify-send")
        .args(["-a", "Steamless Controller", "-i", "input-gaming", title, body])
        .spawn();
}

// ---------------------------------------------------------------- engine

struct SlotHandle {
    running: Arc<AtomicBool>,
    live: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

pub fn spawn_engine(shared: Arc<Shared>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("controller-manager".into())
        .spawn(move || Engine::new(shared).run())
        .expect("spawn controller manager")
}

struct Engine {
    shared: Arc<Shared>,
    slots: HashMap<PathBuf, SlotHandle>,
    km: Arc<Mutex<Option<UinputDevice>>>,
    want: bool,
    active: bool,
    devices: Vec<HidInfo>,
    steam: SteamState,
    steam_pending: SteamState,
    steam_pending_polls: u32,
    active_game: Option<String>,
    release_at: Option<Instant>,
    last_sync: Option<Instant>,
    last_scan: Option<Instant>,
    last_scan_count: u32,
    open_errors: HashMap<PathBuf, String>,
    started: Instant,
    last_beat: Instant,
}

impl Engine {
    fn new(shared: Arc<Shared>) -> Self {
        Self {
            shared,
            slots: HashMap::new(),
            km: Arc::new(Mutex::new(None)),
            want: false,
            active: false,
            devices: Vec::new(),
            steam: SteamState::NoSteam,
            steam_pending: SteamState::NoSteam,
            steam_pending_polls: 0,
            active_game: None,
            release_at: None,
            last_sync: None,
            last_scan: None,
            last_scan_count: 0,
            open_errors: HashMap::new(),
            started: Instant::now(),
            last_beat: Instant::now(),
        }
    }

    fn run(mut self) {
        eventlog::write("START: Steamless Controller for Linux");
        if !self.shared.passive.load(Ordering::SeqCst) {
            let mut s = self.shared.settings.write().unwrap();
            // Manual mode starts on only when "Enable on Launch" is set.
            s.manual_on = s.enable_on_launch;
        }
        loop {
            let cmds: Vec<Command> = std::mem::take(&mut *self.shared.commands.lock().unwrap());
            let mut quit = false;
            for c in cmds {
                match c {
                    Command::ToggleManual => {
                        let mut s = self.shared.settings.write().unwrap();
                        // Toggling by hand always means Manual, as the tray did.
                        let on = !self.active;
                        s.auto_mode = AutoMode::Manual;
                        s.manual_on = on;
                        s.save();
                    }
                    Command::SetManual(on) => {
                        let mut s = self.shared.settings.write().unwrap();
                        s.manual_on = on;
                        s.save();
                    }
                    Command::SetAutoMode(m) => {
                        let mut s = self.shared.settings.write().unwrap();
                        s.auto_mode = m;
                        s.save();
                        eventlog::write(format!("AUTO: mode set to {m:?}"));
                    }
                    Command::SettingsChanged => {}
                    Command::Quit => quit = true,
                }
            }
            if quit || self.shared.quit.load(Ordering::SeqCst) {
                self.release_all();
                break;
            }
            let now = Instant::now();
            if self.last_scan.is_none_or(|t| now - t >= PROCESS_SCAN) {
                self.last_scan = Some(now);
                let procs = games::scan_processes();
                let detected = games::steam_state(&procs);
                self.observe_steam(detected);
                let games = self.shared.settings.read().unwrap().games.clone();
                let g = games::active_profile(&procs, &games);
                if g != self.active_game {
                    eventlog::write(format!("PROFILE: active game {:?}", g));
                    if let Some(key) = &g {
                        if let Some(gp) = games.iter().find(|x| x.matcher.key() == *key) {
                            if !gp.profile.use_default_mappings {
                                notify(&self.shared, "Custom controls loaded",
                                    &format!("Custom controller profile for {} loaded.", gp.name));
                            }
                        }
                    }
                }
                self.active_game = g;
            }
            self.update_profile();
            if self.last_sync.is_none_or(|t| now - t >= DEVICE_SYNC) {
                self.last_sync = Some(now);
                self.sync_devices();
            }
            self.evaluate_control(now);
            self.publish();
            if now - self.last_beat >= HEARTBEAT {
                self.heartbeat(now);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        eventlog::write("EXIT");
    }

    /// TrayApp heartbeat (0x14000b930, timer id 4): a sign of life every 15
    /// minutes, plus a line when the beat came late. Instant follows the
    /// monotonic clock, which stands still during suspend like the original's
    /// unbiased interrupt time, so a closed lid doesn't count as blocked.
    fn heartbeat(&mut self, now: Instant) {
        let since = now - self.last_beat;
        self.last_beat = now;
        if since > HEARTBEAT + HEARTBEAT_SLACK {
            eventlog::write(format!(
                "HEARTBEAT: the loop was blocked for {} s (beat due every {} s)",
                since.as_secs(), HEARTBEAT.as_secs()));
        }
        let live = self.slots.values().any(|s| s.live.load(Ordering::SeqCst));
        let mode = self.shared.settings.read().unwrap().auto_mode;
        eventlog::write(heartbeat_line(
            now - self.started,
            !self.devices.is_empty(),
            self.active && live,
            self.steam,
            mode,
            self.want,
        ));
    }

    /// SteamWatcher::PollLoop: more Steam activity (Steam appeared, a game
    /// started) counts at once so the controller is handed over before Steam
    /// needs it; less activity must hold for several polls in a row so a Steam
    /// self-restart or a game relaunch doesn't make ownership flap.
    fn observe_steam(&mut self, now: SteamState) {
        if self.last_scan_count == 0 {
            self.last_scan_count = 1;
            self.steam = now;
            self.steam_pending = now;
            eventlog::write(format!("STEAM: state {now:?}"));
            return;
        }
        if now == self.steam {
            self.steam_pending_polls = 0;
            return;
        }
        if now > self.steam {
            self.steam = now;
            self.steam_pending_polls = 0;
            eventlog::write(format!("STEAM: state {now:?}"));
        } else if now != self.steam_pending {
            self.steam_pending = now;
            self.steam_pending_polls = 1;
        } else {
            self.steam_pending_polls += 1;
            if self.steam_pending_polls >= LESS_STEAM_POLL_COUNT {
                self.steam = now;
                self.steam_pending_polls = 0;
                eventlog::write(format!("STEAM: state {now:?}"));
            }
        }
    }

    fn update_profile(&self) {
        let s = self.shared.settings.read().unwrap();
        let mut prof = s.default_profile.clone();
        if let Some(key) = &self.active_game {
            if let Some(g) = s.games.iter().find(|g| g.matcher.key() == *key) {
                if !g.profile.use_default_mappings {
                    prof = g.profile.clone();
                }
            }
        }
        drop(s);
        // The settings window previews the profile being edited, live.
        if let Some(p) = self.shared.preview.lock().unwrap().clone() {
            prof = p;
        }
        let mut cur = self.shared.profile.write().unwrap();
        if **cur != prof {
            *cur = Arc::new(prof);
        }
    }

    fn want_control_now(&self) -> bool {
        if self.shared.passive.load(Ordering::SeqCst) {
            return false;
        }
        let s = self.shared.settings.read().unwrap();
        match s.auto_mode {
            AutoMode::Manual => s.manual_on,
            AutoMode::OffUnlessProfile => self.active_game.is_some(),
            AutoMode::OffWhileSteam => self.steam == SteamState::NoSteam,
            AutoMode::OffOnlyInGame => self.steam != SteamState::InGame,
        }
    }

    fn evaluate_control(&mut self, now: Instant) {
        let want = self.want_control_now();
        if want {
            self.release_at = None;
            if !self.want {
                eventlog::write(format!("CONTROL: want=1 (steam={:?} profile={:?})", self.steam, self.active_game));
            }
            self.want = true;
            if !self.active {
                self.acquire();
            }
            return;
        }
        if !self.want {
            return;
        }
        let mode = self.shared.settings.read().unwrap().auto_mode;
        let yielding_to_steam = matches!(mode, AutoMode::OffWhileSteam | AutoMode::OffOnlyInGame);
        if mode == AutoMode::Manual || yielding_to_steam {
            self.want = false;
            eventlog::write("CONTROL: want=0, releasing");
            self.release_all();
            return;
        }
        // A profiled game went away: give it a grace period (alt-tab, restarts).
        match self.release_at {
            None => self.release_at = Some(now + RELEASE_GRACE),
            Some(t) if now >= t => {
                self.want = false;
                self.release_at = None;
                eventlog::write("CONTROL: grace over, releasing");
                self.release_all();
            }
            _ => {}
        }
    }

    fn acquire(&mut self) {
        self.active = true;
        if self.km.lock().unwrap().is_none() {
            match uinput::keyboard_mouse() {
                Ok(d) => *self.km.lock().unwrap() = Some(d),
                Err(e) => {
                    eventlog::write(format!("UINPUT: keyboard/mouse device failed: {e}"));
                }
            }
        }
        eventlog::write("CONTROL: Steamless Mode on");
        self.sync_devices();
    }

    fn release_all(&mut self) {
        // ReleaseControl re-arms the notice, so turning it on again says it again.
        self.shared.pad_warned.store(false, Ordering::SeqCst);
        let had = !self.slots.is_empty();
        for (_, mut s) in self.slots.drain() {
            s.running.store(false, Ordering::SeqCst);
            if let Some(t) = s.thread.take() {
                let _ = t.join();
            }
        }
        *self.km.lock().unwrap() = None;
        if self.active || had {
            eventlog::write("CONTROL: Steamless Mode off, controller handed back");
        }
        self.active = false;
    }

    fn sync_devices(&mut self) {
        let devices = hid::enumerate_controllers();
        let was_connected = !self.devices.is_empty();
        if devices.is_empty() && was_connected {
            eventlog::write("DISCONNECT: no controller interfaces left");
        } else if !devices.is_empty() && !was_connected {
            eventlog::write(format!(
                "CONNECT: {} interface(s), {}",
                devices.len(),
                devices[0].transport().label()
            ));
        }
        self.devices = devices;
        // Drop slots whose thread ended (device removed) or whose device went.
        let present: Vec<PathBuf> = self.devices.iter().map(|d| d.dev_path.clone()).collect();
        let gone: Vec<PathBuf> = self
            .slots
            .iter()
            .filter(|(path, s)| !present.contains(path) || s.thread.as_ref().is_some_and(|t| t.is_finished()))
            .map(|(p, _)| p.clone())
            .collect();
        for path in gone {
            if let Some(mut s) = self.slots.remove(&path) {
                s.running.store(false, Ordering::SeqCst);
                if let Some(t) = s.thread.take() {
                    let _ = t.join();
                }
            }
        }
        if !self.active {
            return;
        }
        for info in self.devices.clone() {
            if self.slots.contains_key(&info.dev_path) {
                continue;
            }
            match HidDevice::open(&info.dev_path) {
                Ok(dev) => {
                    self.open_errors.remove(&info.dev_path);
                    let running = Arc::new(AtomicBool::new(true));
                    let live = Arc::new(AtomicBool::new(false));
                    let ctx = SlotCtx {
                        shared: self.shared.clone(),
                        km: self.km.clone(),
                        running: running.clone(),
                        live: live.clone(),
                        info: info.clone(),
                    };
                    let thread = std::thread::Builder::new()
                        .name(format!("read-{}", info.node))
                        .spawn(move || read_loop(dev, ctx))
                        .ok();
                    self.slots.insert(info.dev_path.clone(), SlotHandle { running, live, thread });
                }
                Err(e) => {
                    let msg = format!("{}: {e}", info.dev_path.display());
                    if self.open_errors.get(&info.dev_path) != Some(&msg) {
                        eventlog::write(format!("OPEN: failed {msg}"));
                        if e.kind() == std::io::ErrorKind::PermissionDenied {
                            notify(
                                &self.shared,
                                "Could not take the controller",
                                "Linux won't let this app open the controller. See \"Setup\" in the window.",
                            );
                        }
                        self.open_errors.insert(info.dev_path.clone(), msg);
                    }
                }
            }
        }
    }

    fn publish(&self) {
        let live_slots = self.slots.values().filter(|s| s.live.load(Ordering::SeqCst)).count();
        let others = if self.devices.is_empty() {
            Vec::new()
        } else {
            let nodes: Vec<PathBuf> = self.devices.iter().map(|d| d.dev_path.clone()).collect();
            hid::holders(&nodes)
        };
        let mut st = self.shared.status.lock().unwrap();
        let old_game = st.game_mode;
        st.connected = !self.devices.is_empty();
        st.transport = self.devices.first().map(|d| d.transport());
        st.game_mode = self.active && live_slots > 0;
        st.live_slots = live_slots;
        st.want_control = self.want;
        st.steam = Some(self.steam);
        st.active_game = self.active_game.as_ref().and_then(|k| {
            let s = self.shared.settings.read().unwrap();
            s.games.iter().find(|g| g.matcher.key() == *k).map(|g| g.name.clone())
        });
        st.others_holding = others;
        st.platform = Some(self.shared.current_profile().platform);
        st.access_error = self.open_errors.values().next().cloned();
        let changed = old_game != st.game_mode;
        drop(st);
        if changed {
            self.shared.request_repaint();
        }
    }
}

// ---------------------------------------------------------------- read loop

struct SlotCtx {
    shared: Arc<Shared>,
    km: Arc<Mutex<Option<UinputDevice>>>,
    running: Arc<AtomicBool>,
    live: Arc<AtomicBool>,
    info: HidInfo,
}

const BINDABLE: usize = 18;

struct Effect {
    strong: u16,
    weak: u16,
    length_ms: u16,
}

struct Slot {
    dev: HidDevice,
    pad: Option<VirtualPad>,
    transport: Transport,
    prev: [u8; 64],
    has_prev: bool,
    held: [Binding; BINDABLE],
    repeat_at: [Option<Instant>; BINDABLE],
    left_pad: TrackpadInput,
    right_pad: TrackpadInput,
    left_press: PressState,
    right_press: PressState,
    left_tap: TapState,
    right_tap: TapState,
    left_trig: TriggerFullPress,
    right_trig: TriggerFullPress,
    left_haptics: PadHaptics,
    right_haptics: PadHaptics,
    prev_dirs: [u8; 2],
    prev_tap: [bool; 2],
    prev_pressed: [bool; 2],
    prev_trig_full: [bool; 2],
    last_pad_haptic: [Option<Instant>; 2],
    rumble: RumbleModel,
    effects: HashMap<i16, Effect>,
    playing: Option<(i16, Option<Instant>)>,
    gain: u16,
    last_rumble_sent: Option<Instant>,
    driving: bool,
    pending_click: [bool; 2],
    power_chord_down: bool,
    power_off_at: Option<Instant>,
    wheel_acc: (i32, i32),
}

fn send_feature(dev: &HidDevice, cmd: u8, payload: &[u8]) -> std::io::Result<()> {
    dev.send_feature(&p::build_cmd(cmd, payload))
}

fn lizard_off(dev: &HidDevice) -> std::io::Result<()> {
    send_feature(dev, p::CMD_CLEAR_DIGITAL_MAPPINGS, &[])?;
    send_feature(dev, p::CMD_SET_SETTINGS, &[p::SETTING_IMU_MODE, 0, 0])?;
    send_feature(dev, p::CMD_SET_SETTINGS, &p::pads_raw_payload())
}

fn keepalive(dev: &HidDevice) -> std::io::Result<()> {
    send_feature(dev, p::CMD_CLEAR_DIGITAL_MAPPINGS, &[])?;
    send_feature(dev, p::CMD_SET_SETTINGS, &p::pads_raw_payload())
}

/// ReadLoop (0x14001a8f8): after the first failed keepalive, the failure is
/// logged again only at try 3, 15 and 60 (bit mask 0x1000000000008008).
/// The heartbeat's "alive" line, fields in the original's order.
pub fn heartbeat_line(up: Duration, connected: bool, game_mode: bool, steam: SteamState,
                      mode: AutoMode, want: bool) -> String {
    let m = up.as_secs() / 60;
    format!(
        "HEARTBEAT: alive uptime={}h{:02}m controller={} gameMode={} steam={} (0=none 1=idle 2=inGame) mode={} wantControl={}",
        m / 60, m % 60,
        if connected { "connected" } else { "absent" },
        if game_mode { "on" } else { "off" },
        steam as i32, mode as i32, want as i32)
}

pub fn keepalive_report_due(failures: u32) -> bool {
    matches!(failures, 3 | 15 | 60)
}

/// What the read loop has seen since the first failed keepalive, for the log.
struct KeepaliveWatch {
    failures: u32,
    state_reports: u32,
    other_reports: u32,
    last_state: Instant,
}

fn lizard_on(dev: &HidDevice) -> std::io::Result<()> {
    send_feature(dev, p::CMD_SET_DEFAULT_MAPPINGS, &[])?;
    send_feature(dev, p::CMD_LOAD_DEFAULT_SETTINGS, &[])
}

/// The desktop's own key repeat (KeyInput.cpp:254 KeyRepeatDelay / :262
/// KeyRepeatInterval follow the system setting). Linux: KDE's kcminputrc
/// RepeatDelay (ms) and RepeatRate (per second); without it, the original's
/// fallback (setting 1 → 500 ms; setting 31 → 30 per second).
fn key_repeat() -> (Duration, Duration) {
    use std::sync::OnceLock;
    static CACHE: OnceLock<(Duration, Duration)> = OnceLock::new();
    *CACHE.get_or_init(|| {
        let mut delay = 250 * (1 + 1);
        let mut per_second = 2.5 + (30.0 - 2.5) * (31.0 / 31.0);
        let path = dirs::config_dir().unwrap_or_default().join("kcminputrc");
        if let Ok(text) = std::fs::read_to_string(path) {
            let mut in_kbd = false;
            for l in text.lines() {
                let l = l.trim();
                if l.starts_with('[') {
                    in_kbd = l == "[Keyboard]";
                } else if in_kbd {
                    if let Some(v) = l.strip_prefix("RepeatDelay=") {
                        if let Ok(v) = v.trim().parse::<u64>() {
                            delay = v.clamp(100, 2000);
                        }
                    } else if let Some(v) = l.strip_prefix("RepeatRate=") {
                        if let Ok(v) = v.trim().parse::<f64>() {
                            per_second = v.clamp(1.0, 100.0);
                        }
                    }
                }
            }
        }
        // KeyRepeatInterval (0x140021b90): whole milliseconds,
        // (long)(1000.0 / per_second + 0.5).
        let interval_ms = (1000.0 / per_second + 0.5) as u64;
        (Duration::from_millis(delay), Duration::from_millis(interval_ms.max(1)))
    })
}

fn pad_kind(pl: Platform) -> PadKind {
    match pl {
        Platform::Xbox => PadKind::Xbox,
        Platform::PlayStation => PadKind::PlayStation,
    }
}

fn toggle_touch_keyboard() {
    std::thread::spawn(|| {
        // KDE Plasma's on-screen keyboard over D-Bus, then common standalone ones.
        let kde = std::process::Command::new("busctl")
            .args(["--user", "get-property", "org.kde.KWin", "/VirtualKeyboard", "org.kde.kwin.VirtualKeyboard", "active"])
            .output();
        if let Ok(out) = kde {
            if out.status.success() {
                let active = String::from_utf8_lossy(&out.stdout).contains("true");
                let _ = std::process::Command::new("busctl")
                    .args([
                        "--user", "set-property", "org.kde.KWin", "/VirtualKeyboard",
                        "org.kde.kwin.VirtualKeyboard", "active", "b",
                        if active { "false" } else { "true" },
                    ])
                    .status();
                return;
            }
        }
        for prog in ["onboard", "wvkbd-mobintl", "squeekboard"] {
            if std::process::Command::new(prog).spawn().is_ok() {
                return;
            }
        }
        eventlog::write("TOUCHKEYBOARD: no on-screen keyboard found");
    });
}

/// Sends a keyboard / mouse binding. Returns whether anything was sent.
fn send_desktop_binding(km: &mut Option<UinputDevice>, b: Binding, down: bool) -> bool {
    use uinput::*;
    let Some(dev) = km.as_mut() else {
        return false;
    };
    let v = down as i32;
    match b {
        Binding::Action(Action::LeftMouse) => dev.emit(EV_KEY, BTN_LEFT, v),
        Binding::Action(Action::RightMouse) => dev.emit(EV_KEY, BTN_RIGHT, v),
        Binding::Action(Action::TouchKeyboard) => {
            if down {
                toggle_touch_keyboard();
            }
            return true;
        }
        Binding::Mouse(m) => {
            let code = match m {
                ExtraMouse::Middle => BTN_MIDDLE,
                ExtraMouse::Back => BTN_SIDE,
                ExtraMouse::Forward => BTN_EXTRA,
            };
            dev.emit(EV_KEY, code, v);
        }
        Binding::Key { code, mods } => {
            let modkeys = [(MOD_CTRL, 29u16), (MOD_ALT, 56), (MOD_SHIFT, 42), (MOD_SUPER, 125)];
            if down {
                for (bit, k) in modkeys {
                    if mods & bit != 0 {
                        dev.emit(EV_KEY, k, 1);
                    }
                }
                dev.emit(EV_KEY, code, 1);
            } else {
                dev.emit(EV_KEY, code, 0);
                for (bit, k) in modkeys.iter().rev() {
                    if mods & bit != 0 {
                        dev.emit(EV_KEY, *k, 0);
                    }
                }
            }
        }
        Binding::Action(_) => return false,
    }
    dev.syn();
    true
}

fn apply_action(b: Binding, f: &mut PadFrame) {
    if let Binding::Action(a) = b {
        match a {
            Action::A => f.a = true,
            Action::B => f.b = true,
            Action::X => f.x = true,
            Action::Y => f.y = true,
            Action::LB => f.lb = true,
            Action::RB => f.rb = true,
            Action::LT => f.lt = 255,
            Action::RT => f.rt = 255,
            Action::Up => f.up = true,
            Action::Down => f.down = true,
            Action::Left => f.left = true,
            Action::Right => f.right = true,
            Action::Menu => f.start = true,
            Action::View => f.back = true,
            Action::L3 => f.l3 = true,
            Action::R3 => f.r3 = true,
            Action::Guide => f.guide = true,
            Action::TouchpadClick => f.touch_click = true,
            _ => {}
        }
    }
}

fn detect_capture(cur: &[u8], prev: &[u8]) -> Option<Action> {
    let rose = |i: usize, m: u8| (p::byte(cur, i) & m) != 0 && (p::byte(prev, i) & m) == 0;
    let trig = |i: usize| p::i16_at(cur, i) > 0x2000 && p::i16_at(prev, i) <= 0x2000;
    let checks: [(bool, Action); 16] = [
        (rose(2, p::BTN_A), Action::A),
        (rose(2, p::BTN_B), Action::B),
        (rose(2, p::BTN_X), Action::X),
        (rose(2, p::BTN_Y), Action::Y),
        (rose(4, p::BTN_LB), Action::LB),
        (rose(3, p::BTN_RB), Action::RB),
        (trig(6), Action::LT),
        (trig(8), Action::RT),
        (rose(3, p::BTN_DPAD_UP), Action::Up),
        (rose(3, p::BTN_DPAD_DN), Action::Down),
        (rose(3, p::BTN_DPAD_LT), Action::Left),
        (rose(3, p::BTN_DPAD_RT), Action::Right),
        (rose(2, p::BTN_MENU), Action::Menu),
        (rose(3, p::BTN_VIEW), Action::View),
        (rose(3, p::BTN_LS), Action::L3),
        (rose(2, p::BTN_RS), Action::R3),
    ];
    checks.iter().find(|(hit, _)| *hit).map(|(_, a)| *a)
}

fn normalize_pad_axis(raw: i16, max: i32, invert: bool) -> i32 {
    let mut v = raw as i64;
    if invert {
        v = if v == -32768 { 32767 } else { -v };
    }
    (((v + 32768) * max as i64) / 65535).clamp(0, max as i64) as i32
}

impl Slot {
    fn write_out(&mut self, data: &[u8]) -> bool {
        match self.dev.write_output(data) {
            Ok(_) => true,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    eventlog::write(format!("WRITE: output 0x{:02X} failed: {e}", data[0]));
                }
                false
            }
        }
    }

    fn pad_click(&mut self, left: bool) {
        let side = if left { p::HAPTIC_SIDE_LEFT } else { p::HAPTIC_SIDE_RIGHT };
        self.last_pad_haptic[if left { 0 } else { 1 }] = Some(Instant::now());
        self.write_out(&p::haptic_command_report(side, p::HAPTIC_COMMAND_CLICK));
    }

    /// TickTrackpadMovement (0x14004c2c0): at most one tick per pad every
    /// 50 ms (the time is taken before sending); then report 0x82 [side, 1, 0]
    /// (0x14004bc10). True only when the tick was written, so a skipped or
    /// failed tick keeps its distance and goes out on a later frame.
    fn pad_tick(&mut self, left: bool) -> bool {
        let i = if left { 0 } else { 1 };
        let now = Instant::now();
        if self.last_pad_haptic[i].is_some_and(|t| now - t < PAD_HAPTIC_MIN_GAP) {
            return false;
        }
        self.last_pad_haptic[i] = Some(now);
        let side = if left { p::HAPTIC_SIDE_LEFT } else { p::HAPTIC_SIDE_RIGHT };
        self.write_out(&p::haptic_command_report(side, p::HAPTIC_COMMAND_TICK))
    }

    fn release_held(&mut self, km: &Arc<Mutex<Option<UinputDevice>>>) {
        let mut km = km.lock().unwrap();
        for i in 0..BINDABLE {
            if !self.held[i].is_none() {
                send_desktop_binding(&mut km, self.held[i], false);
                self.held[i] = Binding::NONE;
                self.repeat_at[i] = None;
            }
        }
    }

    fn set_game_rumble(&mut self, strength: u32) {
        let now = Instant::now();
        let (strong, weak) = match self.playing {
            Some((id, until)) if until.is_none_or(|t| t > now) => self
                .effects
                .get(&id)
                .map(|e| (e.strong, e.weak))
                .unwrap_or((0, 0)),
            _ => (0, 0),
        };
        let scale = |v: u16| -> u8 {
            let v = v as u32 * self.gain as u32 / 0xFFFF;
            let v = v * strength.min(100) / 100;
            (v >> 8).min(255) as u8
        };
        let (large, small) = (scale(strong), scale(weak));
        self.rumble.set(large, small, now);
        let (l, r) = self.rumble.frame(now);
        self.write_out(&p::rumble_report(l, r));
        self.last_rumble_sent = Some(now);
        self.driving = l != 0 || r != 0;
    }

    fn handle_ff(&mut self, events: Vec<FfEvent>, strength: u32) {
        let mut changed = false;
        for e in events {
            match e {
                FfEvent::Upload { id, strong, weak, length_ms } => {
                    self.effects.insert(id, Effect { strong, weak, length_ms });
                    if self.playing.is_some_and(|(pid, _)| pid == id) {
                        changed = true;
                    }
                }
                FfEvent::Erase { id } => {
                    self.effects.remove(&id);
                    if self.playing.is_some_and(|(pid, _)| pid == id) {
                        self.playing = None;
                        changed = true;
                    }
                }
                FfEvent::Play { id, count } => {
                    if count > 0 {
                        let len = self.effects.get(&id).map(|e| e.length_ms).unwrap_or(0);
                        let until = if len == 0 {
                            None
                        } else {
                            Some(Instant::now() + Duration::from_millis(len as u64 * count.max(1) as u64))
                        };
                        self.playing = Some((id, until));
                    } else if self.playing.is_some_and(|(pid, _)| pid == id) {
                        self.playing = None;
                    }
                    changed = true;
                }
                FfEvent::Gain(g) => {
                    self.gain = g;
                    changed = true;
                }
            }
        }
        if changed {
            self.set_game_rumble(strength);
        }
    }

    /// The rumble thread's job: re-drive the actuators every 40 ms, play
    /// queued trigger clicks, and send one zero when everything goes quiet.
    fn rumble_tick(&mut self, strength: u32) {
        let now = Instant::now();
        if let Some((_, Some(until))) = self.playing {
            if until <= now {
                self.playing = None;
                self.set_game_rumble(strength);
                return;
            }
        }
        // A new trigger click wakes the rumble loop at once (0x14004b400 signals
        // its condition variable) instead of waiting for the next 40 ms turn.
        let kicked = self.pending_click.iter().any(|&c| c);
        for side in 0..2 {
            if self.pending_click[side] {
                self.pending_click[side] = false;
                let s = if side == 0 { p::HAPTIC_SIDE_LEFT } else { p::HAPTIC_SIDE_RIGHT };
                self.write_out(&p::haptic_command_report(s, p::HAPTIC_COMMAND_CLICK));
            }
        }
        let due = self.last_rumble_sent.is_none_or(|t| now - t >= RUMBLE_PERIOD);
        let buzz_ending = self.rumble.buzz_until.iter().any(|b| b.is_some_and(|t| t <= now && now - t < RUMBLE_PERIOD));
        if !due && !buzz_ending && !kicked {
            return;
        }
        let (l, r) = self.rumble.frame(now);
        if l != 0 || r != 0 {
            self.write_out(&p::rumble_report(l, r));
            self.driving = true;
            self.last_rumble_sent = Some(now);
        } else if self.driving {
            self.write_out(&p::rumble_report(0, 0));
            self.driving = false;
            self.last_rumble_sent = Some(now);
        }
    }

    fn frame(&mut self, buf: &[u8], ctx: &SlotCtx, profile: &Profile) {
        let n = buf.len();
        let shared = &ctx.shared;

        // Steam + Y powers the controller off (not on a cable).
        let chord = n > 4
            && (buf[4] & p::BTN_STEAM) != 0
            && (buf[2] & p::BTN_Y) != 0
            && self.transport != Transport::Wired;
        let pressed_chord = chord && !self.power_chord_down;
        self.power_chord_down = chord;
        if pressed_chord && self.power_off_at.is_none() {
            self.rumble = RumbleModel::default();
            self.write_out(&p::rumble_report(0, 0));
            match send_feature(&self.dev, p::CMD_TURN_OFF_CONTROLLER, b"off!") {
                Ok(()) => {
                    eventlog::write(format!("POWEROFF: Steam+Y, command sent ({})", self.transport.label()));
                    self.power_off_at = Some(Instant::now());
                    self.release_held(&ctx.km);
                    if let Some(pad) = self.pad.as_mut() {
                        pad.release_all();
                    }
                }
                Err(e) => eventlog::write(format!("POWEROFF: command write failed: {e}")),
            }
        }
        if let Some(t) = self.power_off_at {
            if t.elapsed() < POWER_OFF_GRACE {
                self.has_prev = false;
                return;
            }
            self.power_off_at = None;
            eventlog::write("POWEROFF: still reporting after 3 s, resuming input");
        }

        let b2 = p::byte(buf, 2);
        let b3 = p::byte(buf, 3);
        let b4 = p::byte(buf, 4);
        let b5 = p::byte(buf, 5);
        let (lx, ly, la) = (p::i16_at(buf, 18), p::i16_at(buf, 20), p::u16_at(buf, 22));
        let (rx, ry, ra) = (p::i16_at(buf, 24), p::i16_at(buf, 26), p::u16_at(buf, 28));
        let lt_touch = b5 & p::BTN_TP_LT != 0;
        let rt_touch = b4 & p::BTN_TP_RT != 0;

        let lp = self.left_press.update(la, b5 & p::BTN_TP_LT_CLICK != 0);
        let rp = self.right_press.update(ra, b4 & p::BTN_TP_RT_CLICK != 0);

        let lm = self.left_pad.update(&profile.left_pad, lt_touch, lp, lx, ly);
        let rm = self.right_pad.update(&profile.right_pad, rt_touch, rp, rx, ry);
        for (side, m, area, mode) in [("left", &lm, la, profile.left_pad.mode),
                                      ("right", &rm, ra, profile.right_pad.mode)] {
            if let Some(j) = m.jump {
                eventlog::write(format!(
                    "TRACKPAD: {side} pad moved {:.0} units in one frame (dx={} dy={}, {:.1}ms since the last touched frame, area={area}, mode={})",
                    j.dist, j.dx, j.dy, j.since_ms,
                    if mode == PadMode::MousePointer { "pointer" } else { "scroll" }));
            }
        }

        let ltrig = (p::i16_at(buf, 6) as i32).max(0);
        let rtrig = (p::i16_at(buf, 8) as i32).max(0);
        shared.live.trigger[0].store(ltrig, Ordering::Relaxed);
        shared.live.trigger[1].store(rtrig, Ordering::Relaxed);
        for i in 0..4 {
            shared.live.stick[i].store(p::i16_at(buf, 10 + i * 2) as i32, Ordering::Relaxed);
        }

        let left_dirs = self.left_pad.directions();
        let right_dirs = self.right_pad.directions();
        let left_centre = self.left_pad.click_in_centre();
        let right_centre = self.right_pad.click_in_centre();
        let left_tap = self.left_tap.update(lt_touch, lp, lx, ly);
        let right_tap = self.right_tap.update(rt_touch, rp, rx, ry);
        let left_full = self.left_trig.update(ltrig, &profile.left_trigger);
        let right_full = self.right_trig.update(rtrig, &profile.right_trigger);

        // ---- virtual pad
        let want_kind = pad_kind(profile.platform);
        if self.pad.as_ref().is_some_and(|p| p.kind != want_kind) {
            eventlog::write("PAD: platform changed, replugging the virtual controller");
            self.pad = None;
        }
        if self.pad.is_none() {
            match VirtualPad::create(want_kind) {
                Ok(pad) => {
                    eventlog::write(format!("PAD: virtual {want_kind:?} controller connected"));
                    shared.status.lock().unwrap().pad_error = None;
                    shared.pad_warned.store(false, Ordering::SeqCst);
                    self.pad = Some(pad);
                }
                Err(e) => {
                    let msg = format!("Could not create the virtual controller: {e}");
                    let mut st = shared.status.lock().unwrap();
                    if st.pad_error.as_deref() != Some(&msg) {
                        eventlog::write(format!("PAD: {msg}"));
                        st.pad_error = Some(msg);
                    }
                    drop(st);
                    pad_unavailable_notice(shared);
                }
            }
        }
        if n >= 18 {
            let mut f = PadFrame {
                a: b2 & p::BTN_A != 0,
                b: b2 & p::BTN_B != 0,
                x: b2 & p::BTN_X != 0,
                y: b2 & p::BTN_Y != 0,
                lb: b4 & p::BTN_LB != 0,
                rb: b3 & p::BTN_RB != 0,
                start: b2 & p::BTN_MENU != 0,
                back: b3 & p::BTN_VIEW != 0,
                guide: b4 & p::BTN_STEAM != 0,
                l3: b3 & p::BTN_LS != 0,
                r3: b2 & p::BTN_RS != 0,
                up: b3 & p::BTN_DPAD_UP != 0,
                down: b3 & p::BTN_DPAD_DN != 0,
                left: b3 & p::BTN_DPAD_LT != 0,
                right: b3 & p::BTN_DPAD_RT != 0,
                lt: (p::i16_at(buf, 6) as i32 >> 7).clamp(0, 255) as u8,
                rt: (p::i16_at(buf, 8) as i32 >> 7).clamp(0, 255) as u8,
                ..Default::default()
            };
            let (slx, sly) = apply_stick(p::i16_at(buf, 10), p::i16_at(buf, 12), &profile.left_stick);
            let (srx, sry) = apply_stick(p::i16_at(buf, 14), p::i16_at(buf, 16), &profile.right_stick);
            f.lx = slx;
            f.ly = sly;
            f.rx = srx;
            f.ry = sry;
            if b4 & p::BTN_L4 != 0 {
                apply_action(profile.back.l4, &mut f);
            }
            if b4 & p::BTN_L5 != 0 {
                apply_action(profile.back.l5, &mut f);
            }
            if b2 & p::BTN_R4 != 0 {
                apply_action(profile.back.r4, &mut f);
            }
            if b3 & p::BTN_R5 != 0 {
                apply_action(profile.back.r5, &mut f);
            }
            for (pad, tapped, clicked, dirs) in [
                (&profile.left_pad, left_tap, lp && left_centre, left_dirs),
                (&profile.right_pad, right_tap, rp && right_centre, right_dirs),
            ] {
                if tapped {
                    apply_action(pad.effective_touch(), &mut f);
                }
                if clicked {
                    apply_action(pad.effective_click(), &mut f);
                }
                for d in [DIR_UP, DIR_DOWN, DIR_LEFT, DIR_RIGHT] {
                    if dirs & d != 0 {
                        apply_action(pad.effective_dir(d), &mut f);
                    }
                }
            }
            if left_full {
                apply_action(profile.left_trigger.effective_full(), &mut f);
            }
            if right_full {
                apply_action(profile.right_trigger.effective_full(), &mut f);
            }
            if let Some(pad) = self.pad.as_mut() {
                if pad.kind == PadKind::PlayStation {
                    let r_feed = profile.right_pad.feeds_ds4();
                    let l_feed = profile.left_pad.feeds_ds4();
                    let touch_click = (rp && r_feed) || (lp && l_feed) || f.touch_click;
                    let t = |down: bool, x: i16, y: i16| Touch {
                        down,
                        x: if down { normalize_pad_axis(x, 1919, false) } else { 0 },
                        y: if down { normalize_pad_axis(y, 942, true) } else { 0 },
                    };
                    pad.update_touch([t(rt_touch && r_feed, rx, ry), t(lt_touch && l_feed, lx, ly)], touch_click);
                }
                pad.update(&f);
            }
        }

        // ---- desktop motion from pads in pointer / scroll mode
        {
            let dx = lm.dx + rm.dx;
            let dy = lm.dy + rm.dy;
            let wheel = lm.wheel + rm.wheel;
            let hwheel = lm.hwheel + rm.hwheel;
            if dx != 0 || dy != 0 || wheel != 0 || hwheel != 0 {
                let mut km = ctx.km.lock().unwrap();
                if let Some(dev) = km.as_mut() {
                    use uinput::*;
                    if dx != 0 {
                        dev.emit(EV_REL, REL_X, dx);
                    }
                    if dy != 0 {
                        dev.emit(EV_REL, REL_Y, dy);
                    }
                    if wheel != 0 {
                        dev.emit(EV_REL, REL_WHEEL_HI_RES, wheel);
                        self.wheel_acc.0 += wheel;
                        let notches = self.wheel_acc.0 / 120;
                        if notches != 0 {
                            dev.emit(EV_REL, REL_WHEEL, notches);
                            self.wheel_acc.0 -= notches * 120;
                        }
                    }
                    if hwheel != 0 {
                        dev.emit(EV_REL, REL_HWHEEL_HI_RES, hwheel);
                        self.wheel_acc.1 += hwheel;
                        let notches = self.wheel_acc.1 / 120;
                        if notches != 0 {
                            dev.emit(EV_REL, REL_HWHEEL, notches);
                            self.wheel_acc.1 -= notches * 120;
                        }
                    }
                    dev.syn();
                }
            }
        }

        // ---- trigger click haptic
        if self.has_prev {
            let now = Instant::now();
            for (side, t, cur, prev) in [
                (0usize, &profile.left_trigger, left_full, self.prev_trig_full[0]),
                (1usize, &profile.right_trigger, right_full, self.prev_trig_full[1]),
            ] {
                if cur != prev && (if cur { t.clicks_on_press() } else { t.clicks_on_release() })
                    && self.rumble.queue_trigger_click(side, now)
                {
                    self.pending_click[side] = true;
                }
            }
        }

        // ---- trackpad haptics
        if n >= 28 {
            let r_dpad = profile.right_pad.is_dpad();
            let l_dpad = profile.left_pad.is_dpad();
            if r_dpad && self.prev_dirs[1] != 0 && right_dirs != 0 && self.prev_dirs[1] != right_dirs {
                self.pad_tick(false);
            }
            if l_dpad && self.prev_dirs[0] != 0 && left_dirs != 0 && self.prev_dirs[0] != left_dirs {
                self.pad_tick(true);
            }
            let r_dist = !r_dpad && profile.right_pad.mode != PadMode::SingleButton;
            let l_dist = !l_dpad && profile.left_pad.mode != PadMode::SingleButton;
            let mut rh = std::mem::take(&mut self.right_haptics);
            let out_r = rh.update(rt_touch, rp, rx, ry, ra, r_dist, || self.pad_tick(false));
            self.right_haptics = rh;
            let mut lh = std::mem::take(&mut self.left_haptics);
            let out_l = lh.update(lt_touch, lp, lx, ly, la, l_dist, || self.pad_tick(true));
            self.left_haptics = lh;
            if out_r.click {
                self.pad_click(false);
            }
            if out_l.click {
                self.pad_click(true);
            }
        }

        // ---- keyboard / mouse bindings, edge dispatched
        if self.has_prev {
            let prev = self.prev;
            let lpad = &profile.left_pad;
            let rpad = &profile.right_pad;
            let (lpd, rpd) = (self.prev_dirs[0], self.prev_dirs[1]);
            let edges: [(bool, bool, Binding); BINDABLE] = [
                (b4 & p::BTN_L4 != 0, prev[4] & p::BTN_L4 != 0, profile.back.l4),
                (b4 & p::BTN_L5 != 0, prev[4] & p::BTN_L5 != 0, profile.back.l5),
                (b2 & p::BTN_R4 != 0, prev[2] & p::BTN_R4 != 0, profile.back.r4),
                (b3 & p::BTN_R5 != 0, prev[3] & p::BTN_R5 != 0, profile.back.r5),
                (lp && left_centre, self.prev_pressed[0] && left_centre, lpad.effective_click()),
                (rp && right_centre, self.prev_pressed[1] && right_centre, rpad.effective_click()),
                (left_tap, self.prev_tap[0], lpad.effective_touch()),
                (right_tap, self.prev_tap[1], rpad.effective_touch()),
                (left_dirs & DIR_UP != 0, lpd & DIR_UP != 0, lpad.effective_dir(DIR_UP)),
                (left_dirs & DIR_DOWN != 0, lpd & DIR_DOWN != 0, lpad.effective_dir(DIR_DOWN)),
                (left_dirs & DIR_LEFT != 0, lpd & DIR_LEFT != 0, lpad.effective_dir(DIR_LEFT)),
                (left_dirs & DIR_RIGHT != 0, lpd & DIR_RIGHT != 0, lpad.effective_dir(DIR_RIGHT)),
                (right_dirs & DIR_UP != 0, rpd & DIR_UP != 0, rpad.effective_dir(DIR_UP)),
                (right_dirs & DIR_DOWN != 0, rpd & DIR_DOWN != 0, rpad.effective_dir(DIR_DOWN)),
                (right_dirs & DIR_LEFT != 0, rpd & DIR_LEFT != 0, rpad.effective_dir(DIR_LEFT)),
                (right_dirs & DIR_RIGHT != 0, rpd & DIR_RIGHT != 0, rpad.effective_dir(DIR_RIGHT)),
                (left_full, self.prev_trig_full[0], profile.left_trigger.effective_full()),
                (right_full, self.prev_trig_full[1], profile.right_trigger.effective_full()),
            ];
            let mut km = ctx.km.lock().unwrap();
            for (i, (cur, was, binding)) in edges.iter().enumerate() {
                if cur == was {
                    continue;
                }
                if *cur {
                    if send_desktop_binding(&mut km, *binding, true) {
                        self.held[i] = *binding;
                        self.repeat_at[i] = Some(Instant::now() + key_repeat().0);
                    }
                } else {
                    send_desktop_binding(&mut km, self.held[i], false);
                    self.held[i] = Binding::NONE;
                    self.repeat_at[i] = None;
                }
            }
            // Auto-repeat held keys (the base key only, modifiers stay down).
            let now = Instant::now();
            for i in 0..BINDABLE {
                if let Binding::Key { code, .. } = self.held[i] {
                    if self.repeat_at[i].is_some_and(|t| now >= t) {
                        if let Some(dev) = km.as_mut() {
                            dev.emit(uinput::EV_KEY, code, 2);
                            dev.syn();
                        }
                        self.repeat_at[i] = Some(now + key_repeat().1);
                    }
                }
            }
        }

        // ---- press-to-bind
        if self.has_prev && shared.capturing.load(Ordering::SeqCst) {
            if let Some(a) = detect_capture(buf, &self.prev) {
                shared.capturing.store(false, Ordering::SeqCst);
                *shared.captured.lock().unwrap() = Some(Binding::Action(a));
                shared.request_repaint();
            }
        }

        self.prev_dirs = [left_dirs, right_dirs];
        self.prev_tap = [left_tap, right_tap];
        self.prev_pressed = [lp, rp];
        self.prev_trig_full = [left_full, right_full];
        self.prev = [0; 64];
        self.prev[..n.min(64)].copy_from_slice(&buf[..n.min(64)]);
        self.has_prev = true;
    }
}

fn read_loop(dev: HidDevice, ctx: SlotCtx) {
    let transport = ctx.info.transport();
    let mut slot = Slot {
        dev,
        pad: None,
        transport,
        prev: [0; 64],
        has_prev: false,
        held: [Binding::NONE; BINDABLE],
        repeat_at: [None; BINDABLE],
        left_pad: TrackpadInput::default(),
        right_pad: TrackpadInput::default(),
        left_press: PressState::default(),
        right_press: PressState::default(),
        left_tap: TapState::default(),
        right_tap: TapState::default(),
        left_trig: TriggerFullPress::default(),
        right_trig: TriggerFullPress::default(),
        left_haptics: PadHaptics::default(),
        right_haptics: PadHaptics::default(),
        prev_dirs: [0; 2],
        prev_tap: [false; 2],
        prev_pressed: [false; 2],
        prev_trig_full: [false; 2],
        last_pad_haptic: [None; 2],
        rumble: RumbleModel::default(),
        effects: HashMap::new(),
        playing: None,
        gain: 0xFFFF,
        last_rumble_sent: None,
        driving: false,
        pending_click: [false; 2],
        power_chord_down: false,
        power_off_at: None,
        wheel_acc: (0, 0),
    };
    let mut live = false;
    let mut last_keepalive = Instant::now();
    let mut last_report = Instant::now();
    let mut stalled = false;
    let mut ka = KeepaliveWatch { failures: 0, state_reports: 0, other_reports: 0, last_state: Instant::now() };
    let mut logged_shape = false;
    let mut buf = [0u8; 64];

    while ctx.running.load(Ordering::SeqCst) {
        let profile = ctx.shared.current_profile();
        if live && last_keepalive.elapsed() >= KEEPALIVE {
            last_keepalive = Instant::now();
            // ReadLoop 0x14001a737 … 0x14001ab4c: count failures, log the first
            // (and what the controller still sends) and again at 3, 15 and 60
            // tries; log the recovery with the count and start over.
            match keepalive(&slot.dev) {
                Ok(()) => {
                    if ka.failures > 0 {
                        eventlog::write(format!("KEEPALIVE: recovered after {} failed tries", ka.failures));
                        ka.failures = 0;
                    }
                }
                Err(e) => {
                    ka.failures += 1;
                    if ka.failures == 1 {
                        ka.state_reports = 0;
                        ka.other_reports = 0;
                        eventlog::write(format!("KEEPALIVE: send failed (device write error: {e})"));
                    } else if keepalive_report_due(ka.failures) {
                        eventlog::write(format!(
                            "KEEPALIVE: still failing after {} tries ({e}) — since the first failure {} state report(s) and {} other report(s) arrived, the last state report {:.1}s ago",
                            ka.failures,
                            ka.state_reports,
                            ka.other_reports,
                            ka.last_state.elapsed().as_secs_f64()
                        ));
                    }
                }
            }
        }

        let mut fds = vec![libc::pollfd { fd: slot.dev.raw_fd(), events: libc::POLLIN, revents: 0 }];
        if let Some(pad) = slot.pad.as_ref() {
            fds.push(libc::pollfd { fd: pad.dev.raw_fd(), events: libc::POLLIN, revents: 0 });
        }
        let timeout = if slot.driving || slot.pending_click.iter().any(|c| *c) { 10 } else { 32 };
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
        if fds[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            eventlog::write(format!("DISCONNECT: {} closed by the system", ctx.info.dev_path.display()));
            break;
        }

        let mut gone = false;
        loop {
            match slot.dev.read_now(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    last_report = Instant::now();
                    if stalled {
                        stalled = false;
                        eventlog::write("STALL: reports resumed");
                    }
                    let r = &buf[..n];
                    if p::is_state_report(r[0]) {
                        ka.last_state = Instant::now();
                        ka.state_reports += (ka.failures > 0) as u32;
                    } else {
                        ka.other_reports += (ka.failures > 0) as u32;
                    }
                    if r[0] == p::REPORT_BATTERY_STATUS && n >= 3 {
                        let pct = r[2] as i32;
                        let old = ctx.shared.live.battery.swap(pct, Ordering::Relaxed);
                        if old != pct {
                            eventlog::write(format!("BATTERY: {pct}% (chargeState={})", r[1]));
                            let _ = p::CHARGE_STATE_CHARGING_DONE;
                        }
                        continue;
                    }
                    if !p::is_state_report(r[0]) {
                        continue;
                    }
                    if !live {
                        // The first state report proves this slot has a
                        // controller behind it: now lizard mode can go.
                        match lizard_off(&slot.dev) {
                            Ok(()) => {
                                live = true;
                                ctx.live.store(true, Ordering::SeqCst);
                                last_keepalive = Instant::now();
                                eventlog::write(format!(
                                    "GAMEMODE: lizard mode off on {} ({})",
                                    ctx.info.node,
                                    transport.label()
                                ));
                                ctx.shared.request_repaint();
                            }
                            Err(e) => {
                                eventlog::write(format!("GAMEMODE: lizard-off failed on {}: {e}", ctx.info.node));
                                std::thread::sleep(Duration::from_millis(500));
                                continue;
                            }
                        }
                    }
                    if !logged_shape {
                        logged_shape = true;
                        eventlog::write(format!("REPORT: state id=0x{:02X} len={n}", r[0]));
                    }
                    slot.frame(r, &ctx, &profile);
                }
                Err(e) => {
                    eventlog::write(format!("READ: {} failed: {e}", ctx.info.dev_path.display()));
                    gone = true;
                    break;
                }
            }
        }
        if gone {
            break;
        }

        if let Some(pad) = slot.pad.as_mut() {
            let events = pad.dev.poll_ff();
            if !events.is_empty() {
                slot.handle_ff(events, profile.rumble);
            }
        }
        if live {
            slot.rumble_tick(profile.rumble);
        }

        if live && !stalled && last_report.elapsed() > STALL {
            stalled = true;
            eventlog::write("STALL: no reports for 4s");
            if slot.power_off_at.is_none() {
                let pct = ctx.shared.live.battery.load(Ordering::Relaxed);
                let body = if pct >= 0 {
                    format!("No input received for several seconds — the battery may be dead, the controller may have gone to sleep, or the wireless signal dropped. Last battery report: {pct}%.")
                } else {
                    "No input received for several seconds — the battery may be dead, the controller may have gone to sleep, or the wireless signal dropped.".to_string()
                };
                notify(&ctx.shared, "Controller not responding", &body);
            }
        }
    }

    // Leaving: let go of everything and give the controller its lizard mode back.
    slot.release_held(&ctx.km);
    if let Some(pad) = slot.pad.as_mut() {
        pad.release_all();
    }
    slot.pad = None;
    if live {
        slot.write_out(&p::rumble_report(0, 0));
        if let Err(e) = lizard_on(&slot.dev) {
            eventlog::write(format!("GAMEMODE: lizard-on failed on {}: {e}", ctx.info.node));
        }
    }
    ctx.live.store(false, Ordering::SeqCst);
    ctx.shared.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guide / PS Button and PS Touchpad Click as bindable outputs (upstream #121).
    #[test]
    fn heartbeat_line_matches_program() {
        let l = heartbeat_line(Duration::from_secs(3 * 3600 + 7 * 60 + 59), true, false,
            SteamState::InGame, AutoMode::OffOnlyInGame, true);
        assert_eq!(l, "HEARTBEAT: alive uptime=3h07m controller=connected gameMode=off steam=2 (0=none 1=idle 2=inGame) mode=2 wantControl=1");
        assert_eq!(HEARTBEAT.as_millis(), 0xdbba0);
        assert_eq!((HEARTBEAT + HEARTBEAT_SLACK).as_millis(), 0xea600);
    }

    #[test]
    fn guide_and_touchpad_click_bind() {
        let mut f = PadFrame::default();
        assert!(keepalive_report_due(3) && keepalive_report_due(15) && keepalive_report_due(60));
        assert!(!keepalive_report_due(1) && !keepalive_report_due(2) && !keepalive_report_due(61));
        apply_action(Binding::Action(Action::Guide), &mut f);
        apply_action(Binding::Action(Action::TouchpadClick), &mut f);
        assert!(f.guide && f.touch_click);
        assert_eq!(Action::Guide.label(true), "PS Button");
        let json = serde_json::to_string(&Binding::Action(Action::TouchpadClick)).unwrap();
        assert_eq!(serde_json::from_str::<Binding>(&json).unwrap(), Binding::Action(Action::TouchpadClick));
    }
}
