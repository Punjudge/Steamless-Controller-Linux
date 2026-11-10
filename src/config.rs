//! Profiles, bindings and app settings, with the original's defaults and
//! limits (MECHANICS.md). Stored as JSON in ~/.config/steamless-controller.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------- bindings

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Action {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    LT,
    RT,
    Up,
    Down,
    Left,
    Right,
    LeftMouse,
    RightMouse,
    None,
    Menu,
    View,
    L3,
    R3,
    TouchKeyboard,
    /// Xbox Guide / PlayStation button (upstream issue #121, PR #145).
    Guide,
    /// PlayStation touchpad click; nothing in Xbox mode (upstream #121, #145).
    TouchpadClick,
}

impl Action {
    pub const GAMEPAD: [Action; 16] = [
        Action::A,
        Action::B,
        Action::X,
        Action::Y,
        Action::LB,
        Action::RB,
        Action::LT,
        Action::RT,
        Action::Up,
        Action::Down,
        Action::Left,
        Action::Right,
        Action::Menu,
        Action::View,
        Action::L3,
        Action::R3,
    ];

    pub fn label(self, ps: bool) -> &'static str {
        match (self, ps) {
            (Action::A, false) => "A",
            (Action::B, false) => "B",
            (Action::X, false) => "X",
            (Action::Y, false) => "Y",
            (Action::A, true) => "Cross",
            (Action::B, true) => "Circle",
            (Action::X, true) => "Square",
            (Action::Y, true) => "Triangle",
            (Action::LB, false) => "LB",
            (Action::RB, false) => "RB",
            (Action::LT, false) => "LT",
            (Action::RT, false) => "RT",
            (Action::LB, true) => "L1",
            (Action::RB, true) => "R1",
            (Action::LT, true) => "L2",
            (Action::RT, true) => "R2",
            (Action::Up, _) => "D-pad Up",
            (Action::Down, _) => "D-pad Down",
            (Action::Left, _) => "D-pad Left",
            (Action::Right, _) => "D-pad Right",
            (Action::LeftMouse, _) => "Left Mouse Button",
            (Action::RightMouse, _) => "Right Mouse Button",
            (Action::None, _) => "Off",
            (Action::Menu, false) => "Menu",
            (Action::View, false) => "View",
            (Action::Menu, true) => "Options",
            (Action::View, true) => "Share",
            (Action::L3, _) => "L3",
            (Action::R3, _) => "R3",
            (Action::TouchKeyboard, _) => "On-screen Keyboard",
            (Action::Guide, false) => "Guide",
            (Action::Guide, true) => "PS Button",
            (Action::TouchpadClick, false) => "Touchpad Click (PlayStation only)",
            (Action::TouchpadClick, true) => "Touchpad Click",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ExtraMouse {
    Middle,
    Back,
    Forward,
}

impl ExtraMouse {
    pub fn label(self) -> &'static str {
        match self {
            ExtraMouse::Middle => "Middle Mouse Button",
            ExtraMouse::Back => "Mouse Back (thumb)",
            ExtraMouse::Forward => "Mouse Forward (thumb)",
        }
    }
}

pub const MOD_CTRL: u8 = 1;
pub const MOD_ALT: u8 = 2;
pub const MOD_SHIFT: u8 = 4;
pub const MOD_SUPER: u8 = 8;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Binding {
    Action(Action),
    /// A Linux key code (input-event-codes.h) and Ctrl/Alt/Shift/Super bits.
    Key { code: u16, mods: u8 },
    Mouse(ExtraMouse),
}

impl Default for Binding {
    fn default() -> Self {
        Binding::Action(Action::None)
    }
}

impl Binding {
    pub const NONE: Binding = Binding::Action(Action::None);

    pub fn is_none(&self) -> bool {
        *self == Binding::NONE
    }

    pub fn label(&self, ps: bool) -> String {
        match self {
            Binding::Action(a) => a.label(ps).to_string(),
            Binding::Mouse(m) => m.label().to_string(),
            Binding::Key { code, mods } => {
                let mut s = String::new();
                if mods & MOD_CTRL != 0 {
                    s.push_str("Ctrl + ");
                }
                if mods & MOD_ALT != 0 {
                    s.push_str("Alt + ");
                }
                if mods & MOD_SHIFT != 0 {
                    s.push_str("Shift + ");
                }
                if mods & MOD_SUPER != 0 {
                    s.push_str("Super + ");
                }
                s.push_str(key_name(*code));
                s
            }
        }
    }
}

/// Linux key codes offered in the key picker, with their names.
pub const KEYS: &[(u16, &str)] = &[
    (30, "A"), (48, "B"), (46, "C"), (32, "D"), (18, "E"), (33, "F"), (34, "G"), (35, "H"),
    (23, "I"), (36, "J"), (37, "K"), (38, "L"), (50, "M"), (49, "N"), (24, "O"), (25, "P"),
    (16, "Q"), (19, "R"), (31, "S"), (20, "T"), (22, "U"), (47, "V"), (17, "W"), (45, "X"),
    (21, "Y"), (44, "Z"),
    (11, "0"), (2, "1"), (3, "2"), (4, "3"), (5, "4"), (6, "5"), (7, "6"), (8, "7"), (9, "8"), (10, "9"),
    (59, "F1"), (60, "F2"), (61, "F3"), (62, "F4"), (63, "F5"), (64, "F6"), (65, "F7"),
    (66, "F8"), (67, "F9"), (68, "F10"), (87, "F11"), (88, "F12"),
    (1, "Esc"), (15, "Tab"), (57, "Space"), (28, "Enter"), (14, "Backspace"), (58, "Caps Lock"),
    (103, "Up"), (108, "Down"), (105, "Left"), (106, "Right"),
    (102, "Home"), (107, "End"), (104, "Page Up"), (109, "Page Down"), (110, "Insert"), (111, "Delete"),
    (12, "-"), (13, "="), (26, "["), (27, "]"), (39, ";"), (40, "'"), (41, "`"), (43, "\\"),
    (51, ","), (52, "."), (53, "/"),
    (29, "Left Ctrl"), (42, "Left Shift"), (56, "Left Alt"), (125, "Super"),
    (97, "Right Ctrl"), (54, "Right Shift"), (100, "Right Alt"),
    (99, "Print Screen"), (119, "Pause"), (70, "Scroll Lock"),
    (113, "Mute"), (114, "Volume Down"), (115, "Volume Up"),
    (164, "Play / Pause"), (163, "Next Track"), (165, "Previous Track"), (166, "Stop"),
];

pub fn key_name(code: u16) -> &'static str {
    KEYS.iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| *n)
        .unwrap_or("Key")
}

// ---------------------------------------------------------------- sticks

pub const STICK_RAW_MAX: i32 = 0x7FFF;
pub const STICK_DEADZONE_PERCENT: i32 = 8;
pub const STICK_INNER_DEFAULT: u32 = 8;
pub const STICK_OUTER_DEFAULT: u32 = 100;
pub const STICK_MIN_GAP: u32 = 5;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum StickMode {
    Default,
    Circle,
    Square,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct StickSettings {
    pub mode: StickMode,
    pub inner: u32,
    pub outer: u32,
}

impl Default for StickSettings {
    fn default() -> Self {
        Self { mode: StickMode::Default, inner: STICK_INNER_DEFAULT, outer: STICK_OUTER_DEFAULT }
    }
}

pub fn clamp_stick_range(inner: &mut u32, outer: &mut u32) {
    *inner = (*inner).min(100 - STICK_MIN_GAP);
    *outer = (*outer).clamp(*inner + STICK_MIN_GAP, 100);
}

// ---------------------------------------------------------------- triggers

pub const TRIGGER_RAW_MAX: i32 = 0x7FFF;
pub const TRIGGER_PRESS_DEFAULT: u32 = 95;
pub const TRIGGER_RELEASE_DEFAULT: u32 = 80;
pub const TRIGGER_PRESS_MIN: u32 = 20;
pub const TRIGGER_PRESS_MAX: u32 = 100;
pub const TRIGGER_RELEASE_MIN: u32 = 5;
pub const TRIGGER_HYSTERESIS_MIN: u32 = 3;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TriggerMode {
    Standard,
    DualStage,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum TriggerHaptic {
    Off,
    Press,
    PressAndRelease,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct TriggerSettings {
    pub mode: TriggerMode,
    pub press: u32,
    pub release: u32,
    pub haptic: TriggerHaptic,
    pub full: Binding,
}

impl Default for TriggerSettings {
    fn default() -> Self {
        Self {
            mode: TriggerMode::Standard,
            press: TRIGGER_PRESS_DEFAULT,
            release: TRIGGER_RELEASE_DEFAULT,
            haptic: TriggerHaptic::PressAndRelease,
            full: Binding::NONE,
        }
    }
}

pub fn clamp_trigger_press(v: u32) -> u32 {
    v.clamp(TRIGGER_PRESS_MIN, TRIGGER_PRESS_MAX)
}

pub fn clamp_trigger_release(release: u32, press: u32) -> u32 {
    let ceiling = if press > TRIGGER_HYSTERESIS_MIN {
        press - TRIGGER_HYSTERESIS_MIN
    } else {
        TRIGGER_RELEASE_MIN
    };
    release.clamp(TRIGGER_RELEASE_MIN, ceiling.max(TRIGGER_RELEASE_MIN))
}

impl TriggerSettings {
    pub fn is_dual(&self) -> bool {
        self.mode == TriggerMode::DualStage
    }
    pub fn effective_full(&self) -> Binding {
        if self.is_dual() {
            self.full
        } else {
            Binding::NONE
        }
    }
    pub fn clicks_on_press(&self) -> bool {
        self.is_dual() && self.haptic != TriggerHaptic::Off
    }
    pub fn clicks_on_release(&self) -> bool {
        self.is_dual() && self.haptic == TriggerHaptic::PressAndRelease
    }
}

// ---------------------------------------------------------------- trackpads

pub const SCROLL_SPEED_DEFAULT: u32 = 100;
pub const SCROLL_SPEED_MIN: u32 = 10;
pub const SCROLL_SPEED_MAX: u32 = 400;
pub const RUMBLE_DEFAULT: u32 = 100;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PadMode {
    None,
    MousePointer,
    ScrollWheel,
    Ds4Touchpad,
    DirectionalPad,
    SingleButton,
}

impl PadMode {
    pub fn label(self) -> &'static str {
        match self {
            PadMode::None => "Off",
            PadMode::MousePointer => "Mouse Pointer",
            PadMode::ScrollWheel => "Scroll Wheel",
            PadMode::Ds4Touchpad => "PlayStation Touchpad",
            PadMode::DirectionalPad => "Directional Pad",
            PadMode::SingleButton => "Single Button",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScrollDirection {
    Natural,
    Reversed,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Diagonals {
    EightWay,
    FourWay,
}

pub const DIR_UP: u8 = 1;
pub const DIR_DOWN: u8 = 2;
pub const DIR_LEFT: u8 = 4;
pub const DIR_RIGHT: u8 = 8;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct PadSettings {
    pub mode: PadMode,
    pub scroll_dir: ScrollDirection,
    pub speed: u32,
    pub click: Binding,
    /// The tap binding (called "touch" in the original's settings).
    pub touch: Binding,
    pub up: Binding,
    pub down: Binding,
    pub left: Binding,
    pub right: Binding,
    pub diagonals: Diagonals,
}

impl Default for PadSettings {
    fn default() -> Self {
        Self {
            mode: PadMode::None,
            scroll_dir: ScrollDirection::Natural,
            speed: SCROLL_SPEED_DEFAULT,
            click: Binding::NONE,
            touch: Binding::NONE,
            up: Binding::Action(Action::Up),
            down: Binding::Action(Action::Down),
            left: Binding::Action(Action::Left),
            right: Binding::Action(Action::Right),
            diagonals: Diagonals::EightWay,
        }
    }
}

impl PadSettings {
    pub fn is_dpad(&self) -> bool {
        self.mode == PadMode::DirectionalPad
    }
    pub fn feeds_ds4(&self) -> bool {
        self.mode == PadMode::Ds4Touchpad
    }
    pub fn effective_click(&self) -> Binding {
        if self.feeds_ds4() {
            Binding::NONE
        } else {
            self.click
        }
    }
    pub fn effective_touch(&self) -> Binding {
        if self.mode == PadMode::None || self.feeds_ds4() {
            Binding::NONE
        } else {
            self.touch
        }
    }
    pub fn effective_dir(&self, d: u8) -> Binding {
        if !self.is_dpad() {
            return Binding::NONE;
        }
        match d {
            DIR_UP => self.up,
            DIR_DOWN => self.down,
            DIR_LEFT => self.left,
            DIR_RIGHT => self.right,
            _ => Binding::NONE,
        }
    }
}

// ---------------------------------------------------------------- profiles

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Platform {
    Xbox,
    PlayStation,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct BackButtons {
    pub l4: Binding,
    pub l5: Binding,
    pub r4: Binding,
    pub r5: Binding,
}

impl Default for BackButtons {
    fn default() -> Self {
        Self {
            l4: Binding::Action(Action::LeftMouse),
            l5: Binding::NONE,
            r4: Binding::Action(Action::LeftMouse),
            r5: Binding::NONE,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Profile {
    pub use_default_mappings: bool,
    pub platform: Platform,
    pub back: BackButtons,
    pub left_pad: PadSettings,
    pub right_pad: PadSettings,
    pub left_trigger: TriggerSettings,
    pub right_trigger: TriggerSettings,
    pub rumble: u32,
    pub left_stick: StickSettings,
    pub right_stick: StickSettings,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            use_default_mappings: false,
            platform: Platform::Xbox,
            back: BackButtons::default(),
            left_pad: PadSettings { mode: PadMode::ScrollWheel, ..Default::default() },
            right_pad: PadSettings {
                mode: PadMode::MousePointer,
                click: Binding::Action(Action::LeftMouse),
                ..Default::default()
            },
            left_trigger: TriggerSettings::default(),
            right_trigger: TriggerSettings::default(),
            rumble: RUMBLE_DEFAULT,
            left_stick: StickSettings::default(),
            right_stick: StickSettings::default(),
        }
    }
}

/// How a game is recognised as running on Linux.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum GameMatch {
    /// A Steam game, by app id (seen in Steam's launch command line).
    Steam(u32),
    /// A program, by its file name (e.g. "game.exe" under Proton, or "game").
    Program(String),
}

impl GameMatch {
    pub fn key(&self) -> String {
        match self {
            GameMatch::Steam(id) => format!("steam:{id}"),
            GameMatch::Program(p) => format!("exe:{}", p.to_lowercase()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct GameProfile {
    pub name: String,
    pub matcher: GameMatch,
    pub profile: Profile,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AutoMode {
    Manual,
    OffWhileSteam,
    OffOnlyInGame,
    OffUnlessProfile,
}

impl AutoMode {
    pub const ALL: [AutoMode; 4] = [
        AutoMode::Manual,
        AutoMode::OffWhileSteam,
        AutoMode::OffOnlyInGame,
        AutoMode::OffUnlessProfile,
    ];
    pub fn label(self) -> &'static str {
        match self {
            AutoMode::Manual => "Manual",
            AutoMode::OffWhileSteam => "Auto - Off while Steam running",
            AutoMode::OffOnlyInGame => "Auto - Off ONLY while in Steam game",
            AutoMode::OffUnlessProfile => "Auto - Off unless a game profile is running",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub auto_mode: AutoMode,
    pub notifications: bool,
    pub enable_on_launch: bool,
    /// Manual mode: whether Steamless Mode was asked for.
    pub manual_on: bool,
    pub default_profile: Profile,
    pub games: Vec<GameProfile>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_mode: AutoMode::Manual,
            notifications: true,
            enable_on_launch: false,
            manual_on: false,
            default_profile: Profile::default(),
            games: Vec::new(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("steamless-controller")
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
            .map(|mut s| {
                s.sanitize();
                s
            })
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let dir = config_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let tmp = dir.join("settings.json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(tmp, settings_path());
            }
        }
    }

    pub fn sanitize(&mut self) {
        sanitize_profile(&mut self.default_profile);
        for g in &mut self.games {
            sanitize_profile(&mut g.profile);
        }
    }
}

pub fn sanitize_profile(p: &mut Profile) {
    for pad in [&mut p.left_pad, &mut p.right_pad] {
        pad.speed = pad.speed.clamp(SCROLL_SPEED_MIN, SCROLL_SPEED_MAX);
    }
    for t in [&mut p.left_trigger, &mut p.right_trigger] {
        t.press = clamp_trigger_press(t.press);
        t.release = clamp_trigger_release(t.release, t.press);
    }
    for s in [&mut p.left_stick, &mut p.right_stick] {
        clamp_stick_range(&mut s.inner, &mut s.outer);
    }
    p.rumble = p.rumble.min(100);
}
