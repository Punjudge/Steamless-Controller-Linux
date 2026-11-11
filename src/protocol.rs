//! The 2026 Steam Controller's HID protocol, as documented in MECHANICS.md.

pub const VALVE_VID: u16 = 0x28DE;
pub const PID_WIRED: u16 = 0x1302;
pub const PID_BLUETOOTH: u16 = 0x1303;
pub const PID_PUCK: u16 = 0x1304;
pub const PID_NEREID: u16 = 0x1305;
pub const CONTROLLER_PIDS: [u16; 4] = [PID_WIRED, PID_BLUETOOTH, PID_PUCK, PID_NEREID];

pub const VENDOR_USAGE_PAGE: u16 = 0xFF00;
pub const CONTROLLER_USAGE: u16 = 0x0001;

pub const REPORT_STATE: u8 = 0x45;
pub const REPORT_STATE_LEGACY: u8 = 0x42;
pub const REPORT_BATTERY_STATUS: u8 = 0x43;

pub const CHARGE_STATE_CHARGING_DONE: u8 = 4;

pub const FEATURE_REPORT_CMD: u8 = 0x01;
pub const FEATURE_LEN: usize = 64;

pub const CMD_CLEAR_DIGITAL_MAPPINGS: u8 = 0x81;
pub const CMD_SET_DEFAULT_MAPPINGS: u8 = 0x85;
pub const CMD_SET_SETTINGS: u8 = 0x87;
pub const CMD_LOAD_DEFAULT_SETTINGS: u8 = 0x8E;
pub const CMD_TURN_OFF_CONTROLLER: u8 = 0x9F;

pub const OUT_HAPTIC_RUMBLE: u8 = 0x80;
pub const OUT_HAPTIC_COMMAND: u8 = 0x82;

pub const SETTING_RIGHT_TRACKPAD_MODE: u8 = 0x07;
pub const SETTING_LEFT_TRACKPAD_MODE: u8 = 0x08;
pub const SETTING_IMU_MODE: u8 = 0x30;

pub const HAPTIC_COMMAND_TICK: u8 = 1;
pub const HAPTIC_COMMAND_CLICK: u8 = 2;
pub const HAPTIC_SIDE_LEFT: u8 = 0;
pub const HAPTIC_SIDE_RIGHT: u8 = 1;

// buf[2]
pub const BTN_A: u8 = 0x01;
pub const BTN_B: u8 = 0x02;
pub const BTN_X: u8 = 0x04;
pub const BTN_Y: u8 = 0x08;
pub const BTN_RS: u8 = 0x20;
pub const BTN_MENU: u8 = 0x40;
pub const BTN_R4: u8 = 0x80;
// buf[3]
pub const BTN_R5: u8 = 0x01;
pub const BTN_RB: u8 = 0x02;
pub const BTN_DPAD_DN: u8 = 0x04;
pub const BTN_DPAD_RT: u8 = 0x08;
pub const BTN_DPAD_LT: u8 = 0x10;
pub const BTN_DPAD_UP: u8 = 0x20;
pub const BTN_VIEW: u8 = 0x40;
pub const BTN_LS: u8 = 0x80;
// buf[4]
pub const BTN_STEAM: u8 = 0x01;
pub const BTN_L4: u8 = 0x02;
pub const BTN_L5: u8 = 0x04;
pub const BTN_LB: u8 = 0x08;
pub const BTN_TP_RT: u8 = 0x20;
pub const BTN_TP_RT_CLICK: u8 = 0x40;
// buf[5]
pub const BTN_TP_LT: u8 = 0x02;
pub const BTN_TP_LT_CLICK: u8 = 0x04;

pub fn is_state_report(id: u8) -> bool {
    id == REPORT_STATE || id == REPORT_STATE_LEGACY
}

/// `[0x01, cmd, len, payload...]` padded to 64 bytes (BuildCmd).
pub fn build_cmd(cmd: u8, payload: &[u8]) -> [u8; FEATURE_LEN] {
    let mut buf = [0u8; FEATURE_LEN];
    buf[0] = FEATURE_REPORT_CMD;
    buf[1] = cmd;
    buf[2] = payload.len() as u8;
    buf[3..3 + payload.len()].copy_from_slice(payload);
    buf
}

pub fn pads_raw_payload() -> [u8; 6] {
    [SETTING_LEFT_TRACKPAD_MODE, 0, 0, SETTING_RIGHT_TRACKPAD_MODE, 0, 0]
}

pub fn rumble_report(left: u16, right: u16) -> [u8; 10] {
    let mut b = [0u8; 10];
    b[0] = OUT_HAPTIC_RUMBLE;
    b[4..6].copy_from_slice(&left.to_le_bytes());
    b[7..9].copy_from_slice(&right.to_le_bytes());
    b
}

pub fn haptic_command_report(side: u8, command: u8) -> [u8; 4] {
    [OUT_HAPTIC_COMMAND, side, command, 0]
}

#[inline]
pub fn i16_at(buf: &[u8], at: usize) -> i16 {
    if buf.len() < at + 2 {
        return 0;
    }
    i16::from_le_bytes([buf[at], buf[at + 1]])
}

#[inline]
pub fn u16_at(buf: &[u8], at: usize) -> u16 {
    if buf.len() < at + 2 {
        return 0;
    }
    u16::from_le_bytes([buf[at], buf[at + 1]])
}

#[inline]
pub fn byte(buf: &[u8], at: usize) -> u8 {
    buf.get(at).copied().unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transport {
    Unknown,
    Wired,
    Dongle,
    Bluetooth,
}

impl Transport {
    pub fn from_ids(bus: u16, pid: u16) -> Self {
        if bus == 0x0005 {
            return Transport::Bluetooth;
        }
        match pid {
            PID_PUCK | PID_NEREID => Transport::Dongle,
            PID_WIRED => Transport::Wired,
            PID_BLUETOOTH => Transport::Bluetooth,
            _ => Transport::Unknown,
        }
    }
    /// SteamController::TransportName (0x14004c630), the name in log lines.
    pub fn log_name(self) -> &'static str {
        match self {
            Transport::Wired => "wired",
            Transport::Dongle => "dongle",
            Transport::Bluetooth => "bluetooth",
            Transport::Unknown => "unknown",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Transport::Wired => "USB cable",
            Transport::Dongle => "Wireless puck",
            Transport::Bluetooth => "Bluetooth",
            Transport::Unknown => "Unknown link",
        }
    }
}
