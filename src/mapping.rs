//! The input rules from MECHANICS.md: stick deadzones, dual-stage triggers,
//! trackpad pointer / scroll / directional pad, press and tap detection,
//! trackpad haptics and the rumble model.

use crate::config::*;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- sticks

pub fn stick_circle(x: i16, y: i16, deadzone: i32, outer: i32) -> (i16, i16) {
    if deadzone <= 0 && outer >= STICK_RAW_MAX {
        return (x, y);
    }
    let dz_raw = deadzone.clamp(0, STICK_RAW_MAX - 1);
    let dz = dz_raw as f64;
    let top = outer.clamp(dz_raw + 1, STICK_RAW_MAX) as f64;
    let mag = (x as f64).hypot(y as f64);
    if mag <= dz {
        return (0, 0);
    }
    let full = STICK_RAW_MAX as f64;
    let scale = (mag - dz) / (top - dz) * full / mag;
    let axis = |v: i16| ((v as f64 * scale).round() as i64).clamp(-32768, 32767) as i16;
    (axis(x), axis(y))
}

pub fn stick_square(x: i16, y: i16, deadzone: i32, outer: i32) -> (i16, i16) {
    let dz_raw = deadzone.clamp(0, STICK_RAW_MAX - 1);
    let dz = dz_raw as f64;
    let top = outer.clamp(dz_raw + 1, STICK_RAW_MAX) as f64;
    let full = STICK_RAW_MAX as f64;
    let axis = |v: i16| -> i16 {
        let m = (v as f64).abs();
        if m <= dz {
            return 0;
        }
        let t = (m - dz) / (top - dz);
        if t >= 1.0 {
            return if v < 0 { -32768 } else { 32767 };
        }
        let scaled = (t * full).round() as i64;
        (if v < 0 { -scaled } else { scaled }) as i16
    };
    (axis(x), axis(y))
}

pub fn apply_stick(x: i16, y: i16, s: &StickSettings) -> (i16, i16) {
    if s.mode == StickMode::Default {
        let dz = STICK_RAW_MAX * STICK_DEADZONE_PERCENT / 100;
        return stick_circle(x, y, dz, STICK_RAW_MAX);
    }
    let (mut inner, mut outer) = (s.inner, s.outer);
    clamp_stick_range(&mut inner, &mut outer);
    let inner_raw = STICK_RAW_MAX * inner as i32 / 100;
    let outer_raw = STICK_RAW_MAX * outer as i32 / 100;
    if s.mode == StickMode::Square {
        stick_square(x, y, inner_raw, outer_raw)
    } else {
        stick_circle(x, y, inner_raw, outer_raw)
    }
}

// ---------------------------------------------------------------- triggers

pub fn trigger_raw_at(percent: u32) -> i32 {
    ((percent as u64 * TRIGGER_RAW_MAX as u64 + 99) / 100) as i32
}

#[derive(Default)]
pub struct TriggerFullPress {
    held: bool,
}

impl TriggerFullPress {
    pub fn update(&mut self, raw: i32, s: &TriggerSettings) -> bool {
        if !s.is_dual() {
            self.held = false;
            return false;
        }
        let press = clamp_trigger_press(s.press);
        let release = clamp_trigger_release(s.release, press);
        if !self.held && raw >= trigger_raw_at(press) {
            self.held = true;
        } else if self.held && raw <= trigger_raw_at(release) {
            self.held = false;
        }
        self.held
    }
}

// ---------------------------------------------------------------- trackpads

pub const PAD_RING_RADIUS: f64 = 12000.0;
pub const PAD_PRESS_AREA: u16 = 1800;
pub const PAD_PRESS_FRAMES: i32 = 2;
const SECTOR_HYSTERESIS_DEG: f64 = 8.0;
const SENSITIVITY: f32 = 0.01125;
const SCROLL_SENSITIVITY: f32 = 0.02;

/// Pressed from contact area, not only the firmware click bit.
#[derive(Default)]
pub struct PressState {
    pub pressed: bool,
    above: i32,
    below: i32,
}

impl PressState {
    pub fn update(&mut self, area: u16, click_bit: bool) -> bool {
        let raw = area >= PAD_PRESS_AREA || click_bit;
        if raw {
            self.above += 1;
            self.below = 0;
        } else {
            self.below += 1;
            self.above = 0;
        }
        if !self.pressed && self.above >= PAD_PRESS_FRAMES {
            self.pressed = true;
        } else if self.pressed && self.below >= PAD_PRESS_FRAMES {
            self.pressed = false;
        }
        self.pressed
    }
}

const TAP_MAX_FRAMES: i32 = 50;
const TAP_MAX_TRAVEL: i32 = 3000;
const TAP_PULSE_FRAMES: i32 = 12;

/// A tap: a brief contact that never pressed and did not travel, decided on lift.
#[derive(Default)]
pub struct TapState {
    frames: i32,
    saw_click: bool,
    start: (i16, i16),
    travel: i32,
    pulse: i32,
}

impl TapState {
    pub fn update(&mut self, touching: bool, clicked: bool, x: i16, y: i16) -> bool {
        if !touching {
            if self.frames > 0
                && !self.saw_click
                && self.frames <= TAP_MAX_FRAMES
                && self.travel <= TAP_MAX_TRAVEL
            {
                self.pulse = TAP_PULSE_FRAMES;
            }
            self.frames = 0;
            self.saw_click = false;
            self.travel = 0;
        } else {
            if self.frames == 0 {
                self.start = (x, y);
                self.travel = 0;
            }
            if clicked {
                self.saw_click = true;
            }
            self.frames += 1;
            let dx = x as f64 - self.start.0 as f64;
            let dy = y as f64 - self.start.1 as f64;
            let d = dx.hypot(dy) as i32;
            if d > self.travel {
                self.travel = d;
            }
        }
        if self.pulse > 0 {
            self.pulse -= 1;
            return true;
        }
        false
    }
}

/// What a pad's movement asks the desktop for this frame.
#[derive(Default, Debug, Clone, Copy)]
pub struct DesktopMotion {
    pub dx: i32,
    pub dy: i32,
    /// Hi-res wheel units, 120 per notch; positive = away from the user / right.
    pub wheel: i32,
    pub hwheel: i32,
}

fn angle_delta(a: f64, b: f64) -> f64 {
    let d = (a - b).abs() % 360.0;
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

fn sector_dirs(sector: i32, diag: Diagonals) -> u8 {
    if sector < 0 {
        return 0;
    }
    if diag == Diagonals::FourWay {
        return match sector {
            0 => DIR_RIGHT,
            1 => DIR_UP,
            2 => DIR_LEFT,
            _ => DIR_DOWN,
        };
    }
    match sector {
        0 => DIR_RIGHT,
        1 => DIR_UP | DIR_RIGHT,
        2 => DIR_UP,
        3 => DIR_UP | DIR_LEFT,
        4 => DIR_LEFT,
        5 => DIR_DOWN | DIR_LEFT,
        6 => DIR_DOWN,
        _ => DIR_DOWN | DIR_RIGHT,
    }
}

pub struct TrackpadInput {
    mode: PadMode,
    diagonals: Diagonals,
    touching: bool,
    prev: (i16, i16),
    rem: (f32, f32),
    scroll_rem: (f32, f32),
    dirs: u8,
    sector: i32,
    click_held: bool,
    click_in_centre: bool,
}

impl Default for TrackpadInput {
    fn default() -> Self {
        Self {
            mode: PadMode::None,
            diagonals: Diagonals::EightWay,
            touching: false,
            prev: (0, 0),
            rem: (0.0, 0.0),
            scroll_rem: (0.0, 0.0),
            dirs: 0,
            sector: -1,
            click_held: false,
            click_in_centre: true,
        }
    }
}

impl TrackpadInput {
    fn reset(&mut self) {
        let (mode, diagonals) = (self.mode, self.diagonals);
        *self = Self::default();
        self.mode = mode;
        self.diagonals = diagonals;
    }

    pub fn directions(&self) -> u8 {
        self.dirs
    }
    pub fn click_in_centre(&self) -> bool {
        self.click_in_centre
    }

    fn resolve_sector(&self, angle: f64) -> i32 {
        let four = self.diagonals == Diagonals::FourWay;
        let count = if four { 4 } else { 8 };
        let step = if four { 90.0 } else { 45.0 };
        let half = step / 2.0;
        if self.sector >= 0
            && angle_delta(angle, self.sector as f64 * step) <= half + SECTOR_HYSTERESIS_DEG
        {
            return self.sector;
        }
        let mut s = ((angle + half) / step).floor() as i32 % count;
        if s < 0 {
            s += count;
        }
        s
    }

    fn update_directions(&mut self, clicked: bool, x: i16, y: i16) {
        if !clicked {
            self.click_held = false;
            self.sector = -1;
            self.dirs = 0;
            return;
        }
        let r = (x as f64).hypot(y as f64);
        if !self.click_held {
            self.click_held = true;
            self.click_in_centre = r < PAD_RING_RADIUS;
            self.sector = -1;
        }
        if self.click_in_centre {
            self.dirs = 0;
            return;
        }
        if r >= PAD_RING_RADIUS {
            let mut angle = (y as f64).atan2(x as f64).to_degrees();
            if angle < 0.0 {
                angle += 360.0;
            }
            self.sector = self.resolve_sector(angle);
        }
        self.dirs = sector_dirs(self.sector, self.diagonals);
    }

    /// One frame. `touching` and `pressed` are already worked out; x/y are the
    /// pad's raw position (+Y up).
    pub fn update(
        &mut self,
        s: &PadSettings,
        touching: bool,
        pressed: bool,
        x: i16,
        y: i16,
    ) -> DesktopMotion {
        if s.mode != self.mode {
            self.mode = s.mode;
            self.reset();
        }
        if s.diagonals != self.diagonals {
            self.diagonals = s.diagonals;
            self.sector = -1;
            self.dirs = 0;
        }
        if !s.is_dpad() {
            self.click_in_centre = true;
            self.dirs = 0;
        }
        let mut out = DesktopMotion::default();
        let wants_movement = matches!(s.mode, PadMode::MousePointer | PadMode::ScrollWheel);
        if s.is_dpad() {
            self.update_directions(pressed, x, y);
            return out;
        }
        if !wants_movement {
            return out;
        }
        let speed = s.speed.clamp(SCROLL_SPEED_MIN, SCROLL_SPEED_MAX) as f32 / 100.0;
        if touching && self.touching {
            let dx = x as i32 - self.prev.0 as i32;
            let dy = y as i32 - self.prev.1 as i32;
            if dx != 0 || dy != 0 {
                if s.mode == PadMode::MousePointer {
                    // Pad Y grows upward, screen Y grows downward.
                    let sens = SENSITIVITY * speed;
                    let fx = dx as f32 * sens + self.rem.0;
                    let fy = -dy as f32 * sens + self.rem.1;
                    let ix = fx.trunc();
                    let iy = fy.trunc();
                    self.rem = (fx - ix, fy - iy);
                    out.dx = ix as i32;
                    out.dy = iy as i32;
                } else {
                    let (mut sx, mut sy) = (dx, dy);
                    if s.scroll_dir == ScrollDirection::Natural {
                        sx = -sx;
                        sy = -sy;
                    }
                    let scale = SCROLL_SENSITIVITY * speed;
                    let fy = sy as f32 * scale + self.scroll_rem.1;
                    let fx = sx as f32 * scale + self.scroll_rem.0;
                    let iy = fy.trunc();
                    let ix = fx.trunc();
                    self.scroll_rem = (fx - ix, fy - iy);
                    out.wheel = iy as i32;
                    out.hwheel = ix as i32;
                }
            }
        }
        if touching {
            self.prev = (x, y);
        }
        self.touching = touching;
        out
    }
}

// ---------------------------------------------------------------- pad haptics

const TICK_DISTANCE: f32 = 6500.0;
const MOTION_DEADZONE: f32 = 45.0;
const TICK_MAX_AREA: f32 = 900.0;
const TICK_MAX_STEP: f32 = 4000.0;
const PRESS_CONFIRM_FRAMES: i32 = 2;
const RELEASE_CONFIRM_FRAMES: i32 = 4;
const TOUCH_GRACE_FRAMES: i32 = 12;
const IDLE_RESET_FRAMES: i32 = 30;
const POST_RELEASE_GRACE_FRAMES: i32 = 45;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClickState {
    WaitingForPress,
    WaitingForRelease,
}

/// What one pad's haptics want sent this frame.
#[derive(Default, Clone, Copy)]
pub struct HapticOut {
    pub click: bool,
    pub tick: bool,
}

/// Click and movement-tick haptics for one trackpad (ReadLoop's haptics block).
pub struct PadHaptics {
    state: ClickState,
    true_frames: i32,
    low_frames: i32,
    was_touching: bool,
    prev: (i16, i16),
    accum: f32,
    touch_grace: i32,
    release_grace: i32,
    idle: i32,
}

impl Default for PadHaptics {
    fn default() -> Self {
        Self {
            state: ClickState::WaitingForPress,
            true_frames: 0,
            low_frames: 0,
            was_touching: false,
            prev: (0, 0),
            accum: 0.0,
            touch_grace: 0,
            release_grace: 0,
            idle: 0,
        }
    }
}

impl PadHaptics {
    /// `tick_ok` asks the rate limiter whether a tick may go out now; it
    /// returns false when the previous waveform is still playing.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        touching: bool,
        pressed: bool,
        x: i16,
        y: i16,
        area: u16,
        wants_distance: bool,
        mut tick_ok: impl FnMut() -> bool,
    ) -> HapticOut {
        let mut out = HapticOut::default();
        self.true_frames = if pressed { self.true_frames + 1 } else { 0 };
        self.low_frames = if pressed { 0 } else { self.low_frames + 1 };
        match self.state {
            ClickState::WaitingForPress => {
                if self.true_frames >= PRESS_CONFIRM_FRAMES {
                    out.click = true;
                    self.state = ClickState::WaitingForRelease;
                    self.prev = (x, y);
                    self.accum = 0.0;
                }
            }
            ClickState::WaitingForRelease => {
                if self.low_frames >= RELEASE_CONFIRM_FRAMES {
                    out.click = true;
                    self.state = ClickState::WaitingForPress;
                    self.release_grace = POST_RELEASE_GRACE_FRAMES;
                    self.prev = (x, y);
                    self.accum = 0.0;
                }
            }
        }
        if touching && !self.was_touching {
            self.prev = (x, y);
            self.accum = 0.0;
            self.touch_grace = TOUCH_GRACE_FRAMES;
        }
        if self.touch_grace > 0 {
            self.touch_grace -= 1;
        }
        if self.release_grace > 0 {
            self.release_grace -= 1;
        }
        if wants_distance
            && touching
            && self.was_touching
            && !pressed
            && self.state == ClickState::WaitingForPress
            && self.touch_grace == 0
        {
            let dx = x as f32 - self.prev.0 as f32;
            let dy = y as f32 - self.prev.1 as f32;
            let dist = dx.hypot(dy);
            if self.release_grace > 0 || area as f32 > TICK_MAX_AREA || dist > TICK_MAX_STEP {
                self.prev = (x, y);
            } else if dist >= MOTION_DEADZONE {
                self.idle = 0;
                self.accum += dist;
                if self.accum >= TICK_DISTANCE {
                    if tick_ok() {
                        out.tick = true;
                        self.accum = 0.0;
                    } else {
                        self.accum = TICK_DISTANCE;
                    }
                }
                self.prev = (x, y);
            } else {
                self.idle += 1;
                if self.idle >= IDLE_RESET_FRAMES {
                    self.accum = 0.0;
                    self.prev = (x, y);
                }
            }
        }
        self.was_touching = touching;
        out
    }
}

// ---------------------------------------------------------------- rumble model

/// Trigger click buzz speed (0x5000 in the original).
pub const TRIGGER_CLICK_BUZZ_SPEED: u16 = 20480;
/// Lowest motor speed a non-zero rumble starts at (0x1200 in the original).
pub const RUMBLE_MIN_SPEED: u16 = 4608;
/// Lowest motor speed of the short start-up boost (0x2200 in the original).
pub const RUMBLE_BOOST_MIN_SPEED: u16 = 8704;
pub const TRIGGER_CLICK_BUZZ: Duration = Duration::from_millis(60);
pub const TRIGGER_CLICK_MIN_GAP: Duration = Duration::from_millis(40);
pub const PAD_HAPTIC_MIN_GAP: Duration = Duration::from_millis(50);
pub const RUMBLE_PERIOD: Duration = Duration::from_millis(40);

fn motor_byte_to_unit(v: u8) -> f64 {
    const DZ: f64 = 6.0;
    if v as f64 <= DZ {
        0.0
    } else {
        (v as f64 - DZ) / (255.0 - DZ)
    }
}

fn unit_to_speed(v: f64, minimum: u16) -> u16 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.0 {
        return 0;
    }
    let curved = v.powf(0.68);
    let speed = minimum as f64 + curved * (0xFFFF - minimum) as f64;
    (speed.round() as i64).clamp(0, 0xFFFF) as u16
}

/// The enhanced haptic model (SetRumble) plus trigger-click buzzes.
#[derive(Default)]
pub struct RumbleModel {
    base: (u16, u16),
    boost: (u16, u16),
    boost_until: Option<Instant>,
    last_motor: (u8, u8),
    has_state: bool,
    pub buzz_until: [Option<Instant>; 2],
    pub last_click_at: [Option<Instant>; 2],
}

impl RumbleModel {
    pub fn set(&mut self, large: u8, small: u8, now: Instant) {
        let low = motor_byte_to_unit(large);
        let high = motor_byte_to_unit(small);
        let left_demand = (low * 1.00 + high * 0.30).clamp(0.0, 1.0);
        let right_demand = (low * 0.78 + high * 0.88).clamp(0.0, 1.0);
        let base_l = unit_to_speed(left_demand, RUMBLE_MIN_SPEED);
        let base_r = unit_to_speed(right_demand, RUMBLE_MIN_SPEED);
        let large_rise = if self.has_state { large as i32 - self.last_motor.0 as i32 } else { large as i32 };
        let small_rise = if self.has_state { small as i32 - self.last_motor.1 as i32 } else { small as i32 };
        self.last_motor = (large, small);
        self.has_state = true;
        self.base = (base_l, base_r);
        if base_l == 0 && base_r == 0 {
            self.boost = (0, 0);
            self.boost_until = None;
        } else {
            let large_punch = large_rise.max(0) as f64 / 255.0 * 0.55;
            let small_punch = small_rise.max(0) as f64 / 255.0 * 0.38;
            let punch = large_punch.max(small_punch);
            if punch >= 0.08 {
                let bl = (left_demand + punch * 0.70 + low * 0.08).clamp(0.0, 1.0);
                let br = (right_demand + punch * 0.62 + high * 0.10).clamp(0.0, 1.0);
                self.boost = (unit_to_speed(bl, RUMBLE_BOOST_MIN_SPEED), unit_to_speed(br, RUMBLE_BOOST_MIN_SPEED));
                let ms = 55 + if large >= 180 { 25 } else { 0 };
                self.boost_until = Some(now + Duration::from_millis(ms));
            }
        }
    }

    pub fn frame(&self, now: Instant) -> (u16, u16) {
        let (mut l, mut r) = self.base;
        if self.boost_until.is_some_and(|t| t > now) {
            l = l.max(self.boost.0);
            r = r.max(self.boost.1);
        }
        if self.buzz_until[0].is_some_and(|t| t > now) {
            l = l.max(TRIGGER_CLICK_BUZZ_SPEED);
        }
        if self.buzz_until[1].is_some_and(|t| t > now) {
            r = r.max(TRIGGER_CLICK_BUZZ_SPEED);
        }
        (l, r)
    }

    /// Returns true when a trigger click was accepted (not still playing).
    pub fn queue_trigger_click(&mut self, side: usize, now: Instant) -> bool {
        if let Some(last) = self.last_click_at[side] {
            if now.duration_since(last) < TRIGGER_CLICK_MIN_GAP {
                return false;
            }
        }
        self.last_click_at[side] = Some(now);
        self.buzz_until[side] = Some(now + TRIGGER_CLICK_BUZZ);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_deadzone_cuts_rest_offset() {
        let s = StickSettings::default();
        // 5.9% resting offset, the worst the original measured.
        assert_eq!(apply_stick(1933, 0, &s), (0, 0));
        // Full deflection still reaches full.
        assert_eq!(apply_stick(32767, 0, &s).0, 32767);
    }

    #[test]
    fn trigger_hysteresis() {
        let s = TriggerSettings { mode: TriggerMode::DualStage, ..Default::default() };
        let mut t = TriggerFullPress::default();
        assert!(!t.update(trigger_raw_at(94), &s));
        assert!(t.update(trigger_raw_at(95), &s));
        assert!(t.update(trigger_raw_at(81), &s));
        assert!(!t.update(trigger_raw_at(80), &s));
    }

    #[test]
    fn dpad_ring() {
        let s = PadSettings { mode: PadMode::DirectionalPad, ..Default::default() };
        let mut p = TrackpadInput::default();
        p.update(&s, true, true, 0, 20000);
        assert_eq!(p.directions(), DIR_UP);
        let mut c = TrackpadInput::default();
        c.update(&s, true, true, 100, 100);
        assert_eq!(c.directions(), 0);
        assert!(c.click_in_centre());
    }
}
