//! Virtual input devices through /dev/uinput: the Xbox 360 or PlayStation pad
//! the games see (the ViGEmBus part of the original), and a keyboard + mouse
//! for bindings that send keys, clicks, pointer movement and scrolling.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_MSC: u16 = 0x04;
pub const EV_FF: u16 = 0x15;
pub const EV_UINPUT: u16 = 0x0101;
pub const SYN_REPORT: u16 = 0;

pub const BTN_LEFT: u16 = 0x110;
pub const BTN_RIGHT: u16 = 0x111;
pub const BTN_MIDDLE: u16 = 0x112;
pub const BTN_SIDE: u16 = 0x113;
pub const BTN_EXTRA: u16 = 0x114;
pub const BTN_TOOL_FINGER: u16 = 0x145;
pub const BTN_TOUCH: u16 = 0x14a;
pub const BTN_TOOL_DOUBLETAP: u16 = 0x14d;
pub const BTN_SOUTH: u16 = 0x130;
pub const BTN_EAST: u16 = 0x131;
pub const BTN_NORTH: u16 = 0x133;
pub const BTN_WEST: u16 = 0x134;
pub const BTN_TL: u16 = 0x136;
pub const BTN_TR: u16 = 0x137;
pub const BTN_TL2: u16 = 0x138;
pub const BTN_TR2: u16 = 0x139;
pub const BTN_SELECT: u16 = 0x13a;
pub const BTN_START: u16 = 0x13b;
pub const BTN_MODE: u16 = 0x13c;
pub const BTN_THUMBL: u16 = 0x13d;
pub const BTN_THUMBR: u16 = 0x13e;

pub const ABS_X: u16 = 0x00;
pub const ABS_Y: u16 = 0x01;
pub const ABS_Z: u16 = 0x02;
pub const ABS_RX: u16 = 0x03;
pub const ABS_RY: u16 = 0x04;
pub const ABS_RZ: u16 = 0x05;
pub const ABS_HAT0X: u16 = 0x10;
pub const ABS_HAT0Y: u16 = 0x11;
pub const ABS_MT_SLOT: u16 = 0x2f;
pub const ABS_MT_POSITION_X: u16 = 0x35;
pub const ABS_MT_POSITION_Y: u16 = 0x36;
pub const ABS_MT_TRACKING_ID: u16 = 0x39;

pub const REL_X: u16 = 0x00;
pub const REL_Y: u16 = 0x01;
pub const REL_HWHEEL: u16 = 0x06;
pub const REL_WHEEL: u16 = 0x08;
pub const REL_WHEEL_HI_RES: u16 = 0x0b;
pub const REL_HWHEEL_HI_RES: u16 = 0x0c;

pub const FF_RUMBLE: u16 = 0x50;
pub const FF_GAIN: u16 = 0x60;

const UI_FF_UPLOAD: u16 = 1;
const UI_FF_ERASE: u16 = 2;

const INPUT_PROP_POINTER: i32 = 0x00;
const INPUT_PROP_BUTTONPAD: i32 = 0x02;

const fn ioc(dir: u64, nr: u64, size: u64) -> libc::c_ulong {
    ((dir << 30) | (size << 16) | ((b'U' as u64) << 8) | nr) as libc::c_ulong
}
const UI_DEV_CREATE: libc::c_ulong = ioc(0, 1, 0);
const UI_DEV_DESTROY: libc::c_ulong = ioc(0, 2, 0);
const UI_DEV_SETUP: libc::c_ulong = ioc(1, 3, 92);
const UI_ABS_SETUP: libc::c_ulong = ioc(1, 4, 28);
const UI_SET_EVBIT: libc::c_ulong = ioc(1, 100, 4);
const UI_SET_KEYBIT: libc::c_ulong = ioc(1, 101, 4);
const UI_SET_RELBIT: libc::c_ulong = ioc(1, 102, 4);
const UI_SET_ABSBIT: libc::c_ulong = ioc(1, 103, 4);
const UI_SET_MSCBIT: libc::c_ulong = ioc(1, 104, 4);
const UI_SET_FFBIT: libc::c_ulong = ioc(1, 107, 4);
const UI_SET_PROPBIT: libc::c_ulong = ioc(1, 110, 4);
const UI_BEGIN_FF_UPLOAD: libc::c_ulong = ioc(3, 200, 104);
const UI_END_FF_UPLOAD: libc::c_ulong = ioc(1, 201, 104);
const UI_BEGIN_FF_ERASE: libc::c_ulong = ioc(3, 202, 12);
const UI_END_FF_ERASE: libc::c_ulong = ioc(1, 203, 12);

#[repr(C)]
struct UinputSetup {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
    name: [u8; 80],
    ff_effects_max: u32,
}

#[repr(C)]
struct UinputAbsSetup {
    code: u16,
    _pad: u16,
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

/// struct uinput_ff_upload on 64-bit Linux: request_id, retval, then two
/// 48-byte struct ff_effect (new and old).
#[repr(C, align(8))]
struct UinputFfUpload {
    request_id: u32,
    retval: i32,
    effect: [u8; 48],
    old: [u8; 48],
}

#[repr(C)]
struct UinputFfErase {
    request_id: u32,
    retval: i32,
    effect_id: u32,
}

#[repr(C)]
struct InputEvent {
    sec: i64,
    usec: i64,
    kind: u16,
    code: u16,
    value: i32,
}

pub struct AbsAxis {
    pub code: u16,
    pub min: i32,
    pub max: i32,
    pub fuzz: i32,
    pub flat: i32,
    pub resolution: i32,
}

pub struct DeviceSpec<'a> {
    pub name: &'a str,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
    pub keys: Vec<u16>,
    pub rels: Vec<u16>,
    pub abs: Vec<AbsAxis>,
    pub props: Vec<i32>,
    pub rumble: bool,
    pub msc_scan: bool,
}

/// A game's force-feedback request, decoded from the uinput fd.
#[derive(Debug, Clone, Copy)]
pub enum FfEvent {
    Upload { id: i16, strong: u16, weak: u16, length_ms: u16 },
    Erase { id: i16 },
    Play { id: i16, count: i32 },
    Gain(u16),
}

pub struct UinputDevice {
    file: File,
}

fn set_bit(fd: i32, req: libc::c_ulong, bit: i32) -> io::Result<()> {
    let rc = unsafe { libc::ioctl(fd, req, bit as libc::c_ulong) };
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

impl UinputDevice {
    pub fn create(spec: &DeviceSpec) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open("/dev/uinput")?;
        let fd = file.as_raw_fd();
        if !spec.keys.is_empty() {
            set_bit(fd, UI_SET_EVBIT, EV_KEY as i32)?;
            for &k in &spec.keys {
                set_bit(fd, UI_SET_KEYBIT, k as i32)?;
            }
        }
        if !spec.rels.is_empty() {
            set_bit(fd, UI_SET_EVBIT, EV_REL as i32)?;
            for &r in &spec.rels {
                set_bit(fd, UI_SET_RELBIT, r as i32)?;
            }
        }
        if !spec.abs.is_empty() {
            set_bit(fd, UI_SET_EVBIT, EV_ABS as i32)?;
            for a in &spec.abs {
                set_bit(fd, UI_SET_ABSBIT, a.code as i32)?;
            }
        }
        if spec.msc_scan {
            set_bit(fd, UI_SET_EVBIT, EV_MSC as i32)?;
            set_bit(fd, UI_SET_MSCBIT, 0x04)?;
        }
        for &p in &spec.props {
            set_bit(fd, UI_SET_PROPBIT, p)?;
        }
        if spec.rumble {
            set_bit(fd, UI_SET_EVBIT, EV_FF as i32)?;
            set_bit(fd, UI_SET_FFBIT, FF_RUMBLE as i32)?;
            set_bit(fd, UI_SET_FFBIT, FF_GAIN as i32)?;
        }
        for a in &spec.abs {
            let s = UinputAbsSetup {
                code: a.code,
                _pad: 0,
                value: 0,
                minimum: a.min,
                maximum: a.max,
                fuzz: a.fuzz,
                flat: a.flat,
                resolution: a.resolution,
            };
            let rc = unsafe { libc::ioctl(fd, UI_ABS_SETUP, &s as *const UinputAbsSetup) };
            if rc < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        let mut setup = UinputSetup {
            bustype: 0x03,
            vendor: spec.vendor,
            product: spec.product,
            version: spec.version,
            name: [0; 80],
            ff_effects_max: if spec.rumble { 16 } else { 0 },
        };
        let name = spec.name.as_bytes();
        let n = name.len().min(79);
        setup.name[..n].copy_from_slice(&name[..n]);
        let rc = unsafe { libc::ioctl(fd, UI_DEV_SETUP, &setup as *const UinputSetup) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        let rc = unsafe { libc::ioctl(fd, UI_DEV_CREATE) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { file })
    }

    pub fn raw_fd(&self) -> i32 {
        self.file.as_raw_fd()
    }

    pub fn emit(&mut self, kind: u16, code: u16, value: i32) {
        let ev = InputEvent { sec: 0, usec: 0, kind, code, value };
        let bytes = unsafe {
            std::slice::from_raw_parts(
                &ev as *const InputEvent as *const u8,
                std::mem::size_of::<InputEvent>(),
            )
        };
        let _ = self.file.write(bytes);
    }

    pub fn syn(&mut self) {
        self.emit(EV_SYN, SYN_REPORT, 0);
    }

    /// Drains force-feedback requests waiting on the device. Uploads and
    /// erases are answered here, as the kernel requires.
    pub fn poll_ff(&mut self) -> Vec<FfEvent> {
        let mut out = Vec::new();
        let size = std::mem::size_of::<InputEvent>();
        let mut buf = vec![0u8; size * 16];
        loop {
            let n = match self.file.read(&mut buf) {
                Ok(n) if n > 0 => n,
                _ => break,
            };
            for chunk in buf[..n].chunks_exact(size) {
                let kind = u16::from_ne_bytes([chunk[16], chunk[17]]);
                let code = u16::from_ne_bytes([chunk[18], chunk[19]]);
                let value = i32::from_ne_bytes([chunk[20], chunk[21], chunk[22], chunk[23]]);
                match (kind, code) {
                    (EV_UINPUT, UI_FF_UPLOAD) => {
                        let fd = self.raw_fd();
                        let mut up = UinputFfUpload {
                            request_id: value as u32,
                            retval: 0,
                            effect: [0; 48],
                            old: [0; 48],
                        };
                        unsafe { libc::ioctl(fd, UI_BEGIN_FF_UPLOAD, &mut up as *mut UinputFfUpload) };
                        let e = &up.effect;
                        let etype = u16::from_ne_bytes([e[0], e[1]]);
                        let id = i16::from_ne_bytes([e[2], e[3]]);
                        let length_ms = u16::from_ne_bytes([e[10], e[11]]);
                        let strong = u16::from_ne_bytes([e[16], e[17]]);
                        let weak = u16::from_ne_bytes([e[18], e[19]]);
                        up.retval = if etype == FF_RUMBLE { 0 } else { -libc::EINVAL };
                        unsafe { libc::ioctl(fd, UI_END_FF_UPLOAD, &up as *const UinputFfUpload) };
                        if etype == FF_RUMBLE {
                            out.push(FfEvent::Upload { id, strong, weak, length_ms });
                        }
                    }
                    (EV_UINPUT, UI_FF_ERASE) => {
                        let fd = self.raw_fd();
                        let mut er = UinputFfErase { request_id: value as u32, retval: 0, effect_id: 0 };
                        unsafe { libc::ioctl(fd, UI_BEGIN_FF_ERASE, &mut er as *mut UinputFfErase) };
                        er.retval = 0;
                        unsafe { libc::ioctl(fd, UI_END_FF_ERASE, &er as *const UinputFfErase) };
                        out.push(FfEvent::Erase { id: er.effect_id as i16 });
                    }
                    (EV_FF, FF_GAIN) => out.push(FfEvent::Gain(value as u16)),
                    (EV_FF, id) => out.push(FfEvent::Play { id: id as i16, count: value }),
                    _ => {}
                }
            }
        }
        out
    }
}

impl Drop for UinputDevice {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY);
        }
    }
}

fn stick(code: u16) -> AbsAxis {
    AbsAxis { code, min: -32768, max: 32767, fuzz: 16, flat: 128, resolution: 0 }
}
fn byte_axis(code: u16, flat: i32) -> AbsAxis {
    AbsAxis { code, min: 0, max: 255, fuzz: 0, flat, resolution: 0 }
}
fn hat(code: u16) -> AbsAxis {
    AbsAxis { code, min: -1, max: 1, fuzz: 0, flat: 0, resolution: 0 }
}

/// What the virtual pad reports in one frame, already mapped and merged.
#[derive(Default, Clone, Copy, PartialEq)]
pub struct PadFrame {
    pub a: bool,
    pub b: bool,
    pub x: bool,
    pub y: bool,
    pub lb: bool,
    pub rb: bool,
    pub start: bool,
    pub back: bool,
    pub guide: bool,
    pub l3: bool,
    pub r3: bool,
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub lt: u8,
    pub rt: u8,
    /// Stick axes, +Y up (the controller's own convention).
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
    /// DS4 only: touchpad pressed.
    pub touch_click: bool,
}

#[derive(Default, Clone, Copy, PartialEq)]
pub struct Touch {
    pub down: bool,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PadKind {
    Xbox,
    PlayStation,
}

/// The virtual game controller.
pub struct VirtualPad {
    pub kind: PadKind,
    pub dev: UinputDevice,
    touchpad: Option<UinputDevice>,
    last: Option<PadFrame>,
    last_touch: [Touch; 2],
    tracking: i32,
}

fn neg(v: i16) -> i32 {
    if v == i16::MIN {
        32767
    } else {
        -(v as i32)
    }
}
fn s16_to_u8(v: i32) -> i32 {
    ((v.clamp(-32768, 32767) + 32768) >> 8).clamp(0, 255)
}

impl VirtualPad {
    pub fn create(kind: PadKind) -> io::Result<Self> {
        match kind {
            PadKind::Xbox => {
                let spec = DeviceSpec {
                    name: "Microsoft X-Box 360 pad",
                    vendor: 0x045e,
                    product: 0x028e,
                    version: 0x0110,
                    keys: vec![
                        BTN_SOUTH, BTN_EAST, BTN_NORTH, BTN_WEST, BTN_TL, BTN_TR, BTN_SELECT,
                        BTN_START, BTN_MODE, BTN_THUMBL, BTN_THUMBR,
                    ],
                    rels: vec![],
                    abs: vec![
                        stick(ABS_X),
                        stick(ABS_Y),
                        stick(ABS_RX),
                        stick(ABS_RY),
                        byte_axis(ABS_Z, 0),
                        byte_axis(ABS_RZ, 0),
                        hat(ABS_HAT0X),
                        hat(ABS_HAT0Y),
                    ],
                    props: vec![],
                    rumble: true,
                    msc_scan: false,
                };
                Ok(Self {
                    kind,
                    dev: UinputDevice::create(&spec)?,
                    touchpad: None,
                    last: None,
                    last_touch: [Touch::default(); 2],
                    tracking: 0,
                })
            }
            PadKind::PlayStation => {
                let spec = DeviceSpec {
                    name: "Sony Interactive Entertainment Wireless Controller",
                    vendor: 0x054c,
                    product: 0x09cc,
                    version: 0x8111,
                    keys: vec![
                        BTN_SOUTH, BTN_EAST, BTN_NORTH, BTN_WEST, BTN_TL, BTN_TR, BTN_TL2,
                        BTN_TR2, BTN_SELECT, BTN_START, BTN_MODE, BTN_THUMBL, BTN_THUMBR,
                    ],
                    rels: vec![],
                    abs: vec![
                        byte_axis(ABS_X, 0),
                        byte_axis(ABS_Y, 0),
                        byte_axis(ABS_Z, 0),
                        byte_axis(ABS_RX, 0),
                        byte_axis(ABS_RY, 0),
                        byte_axis(ABS_RZ, 0),
                        hat(ABS_HAT0X),
                        hat(ABS_HAT0Y),
                    ],
                    props: vec![],
                    rumble: true,
                    msc_scan: false,
                };
                let dev = UinputDevice::create(&spec)?;
                let tspec = DeviceSpec {
                    name: "Sony Interactive Entertainment Wireless Controller Touchpad",
                    vendor: 0x054c,
                    product: 0x09cc,
                    version: 0x8111,
                    keys: vec![BTN_LEFT, BTN_TOUCH, BTN_TOOL_FINGER, BTN_TOOL_DOUBLETAP],
                    rels: vec![],
                    abs: vec![
                        AbsAxis { code: ABS_X, min: 0, max: 1919, fuzz: 0, flat: 0, resolution: 44 },
                        AbsAxis { code: ABS_Y, min: 0, max: 942, fuzz: 0, flat: 0, resolution: 44 },
                        AbsAxis { code: ABS_MT_SLOT, min: 0, max: 1, fuzz: 0, flat: 0, resolution: 0 },
                        AbsAxis { code: ABS_MT_TRACKING_ID, min: 0, max: 65535, fuzz: 0, flat: 0, resolution: 0 },
                        AbsAxis { code: ABS_MT_POSITION_X, min: 0, max: 1919, fuzz: 0, flat: 0, resolution: 44 },
                        AbsAxis { code: ABS_MT_POSITION_Y, min: 0, max: 942, fuzz: 0, flat: 0, resolution: 44 },
                    ],
                    props: vec![INPUT_PROP_POINTER, INPUT_PROP_BUTTONPAD],
                    rumble: false,
                    msc_scan: false,
                };
                let touchpad = UinputDevice::create(&tspec).ok();
                Ok(Self {
                    kind,
                    dev,
                    touchpad,
                    last: None,
                    last_touch: [Touch::default(); 2],
                    tracking: 0,
                })
            }
        }
    }

    pub fn update(&mut self, f: &PadFrame) {
        if self.last.as_ref() == Some(f) {
            return;
        }
        let prev = self.last.unwrap_or_default();
        let first = self.last.is_none();
        let d = &mut self.dev;
        let key = |d: &mut UinputDevice, code: u16, now: bool, was: bool| {
            if first || now != was {
                d.emit(EV_KEY, code, now as i32);
            }
        };
        key(d, BTN_SOUTH, f.a, prev.a);
        key(d, BTN_EAST, f.b, prev.b);
        // xpad reports X as BTN_X (= BTN_NORTH) and Y as BTN_Y (= BTN_WEST);
        // hid-playstation reports Triangle as BTN_NORTH and Square as BTN_WEST.
        // Copying each driver is what makes SDL's mappings for these ids work.
        match self.kind {
            PadKind::Xbox => {
                key(d, BTN_NORTH, f.x, prev.x);
                key(d, BTN_WEST, f.y, prev.y);
            }
            PadKind::PlayStation => {
                key(d, BTN_WEST, f.x, prev.x);
                key(d, BTN_NORTH, f.y, prev.y);
                key(d, BTN_TL2, f.lt > 0, prev.lt > 0);
                key(d, BTN_TR2, f.rt > 0, prev.rt > 0);
            }
        }
        key(d, BTN_TL, f.lb, prev.lb);
        key(d, BTN_TR, f.rb, prev.rb);
        key(d, BTN_SELECT, f.back, prev.back);
        key(d, BTN_START, f.start, prev.start);
        key(d, BTN_MODE, f.guide, prev.guide);
        key(d, BTN_THUMBL, f.l3, prev.l3);
        key(d, BTN_THUMBR, f.r3, prev.r3);

        let hx = f.right as i32 - f.left as i32;
        let hy = f.down as i32 - f.up as i32;
        let phx = prev.right as i32 - prev.left as i32;
        let phy = prev.down as i32 - prev.up as i32;
        let abs = |d: &mut UinputDevice, code: u16, now: i32, was: i32| {
            if first || now != was {
                d.emit(EV_ABS, code, now);
            }
        };
        abs(d, ABS_HAT0X, hx, phx);
        abs(d, ABS_HAT0Y, hy, phy);
        abs(d, ABS_Z, f.lt as i32, prev.lt as i32);
        abs(d, ABS_RZ, f.rt as i32, prev.rt as i32);
        match self.kind {
            PadKind::Xbox => {
                // Linux pads report +Y downwards.
                abs(d, ABS_X, f.lx as i32, prev.lx as i32);
                abs(d, ABS_Y, neg(f.ly), neg(prev.ly));
                abs(d, ABS_RX, f.rx as i32, prev.rx as i32);
                abs(d, ABS_RY, neg(f.ry), neg(prev.ry));
            }
            PadKind::PlayStation => {
                abs(d, ABS_X, s16_to_u8(f.lx as i32), s16_to_u8(prev.lx as i32));
                abs(d, ABS_Y, s16_to_u8(neg(f.ly)), s16_to_u8(neg(prev.ly)));
                abs(d, ABS_RX, s16_to_u8(f.rx as i32), s16_to_u8(prev.rx as i32));
                abs(d, ABS_RY, s16_to_u8(neg(f.ry)), s16_to_u8(neg(prev.ry)));
            }
        }
        d.syn();
        self.last = Some(*f);
    }

    /// DS4 touchpad contacts: [0] = right pad (first finger), [1] = left pad,
    /// as the original packs them. Coordinates 0..1919 x 0..942.
    pub fn update_touch(&mut self, touches: [Touch; 2], click: bool) {
        let Some(tp) = self.touchpad.as_mut() else {
            return;
        };
        if touches == self.last_touch && !click && !self.last_touch.iter().any(|t| t.down) {
            return;
        }
        for (slot, t) in touches.iter().enumerate() {
            let was = self.last_touch[slot];
            tp.emit(EV_ABS, ABS_MT_SLOT, slot as i32);
            if t.down && !was.down {
                self.tracking = (self.tracking + 1) & 0xFFFF;
                tp.emit(EV_ABS, ABS_MT_TRACKING_ID, self.tracking);
            } else if !t.down && was.down {
                tp.emit(EV_ABS, ABS_MT_TRACKING_ID, -1);
            }
            if t.down {
                tp.emit(EV_ABS, ABS_MT_POSITION_X, t.x);
                tp.emit(EV_ABS, ABS_MT_POSITION_Y, t.y);
            }
        }
        let count = touches.iter().filter(|t| t.down).count();
        tp.emit(EV_KEY, BTN_TOUCH, (count > 0) as i32);
        tp.emit(EV_KEY, BTN_TOOL_FINGER, (count == 1) as i32);
        tp.emit(EV_KEY, BTN_TOOL_DOUBLETAP, (count == 2) as i32);
        if let Some(first) = touches.iter().find(|t| t.down) {
            tp.emit(EV_ABS, ABS_X, first.x);
            tp.emit(EV_ABS, ABS_Y, first.y);
        }
        tp.emit(EV_KEY, BTN_LEFT, click as i32);
        tp.syn();
        self.last_touch = touches;
    }

    /// Lets go of everything (power-off chord, leaving Steamless Mode).
    pub fn release_all(&mut self) {
        self.update(&PadFrame::default());
        self.update_touch([Touch::default(); 2], false);
    }
}

/// Every key code a binding can send (KEY_ESC..KEY_MICMUTE) plus mouse buttons.
pub fn keyboard_mouse() -> io::Result<UinputDevice> {
    let mut keys: Vec<u16> = (1u16..=248).collect();
    keys.extend([BTN_LEFT, BTN_RIGHT, BTN_MIDDLE, BTN_SIDE, BTN_EXTRA]);
    let spec = DeviceSpec {
        name: "Steamless Controller Keyboard and Mouse",
        vendor: 0x28de,
        product: 0x11fe,
        version: 1,
        keys,
        rels: vec![REL_X, REL_Y, REL_WHEEL, REL_HWHEEL, REL_WHEEL_HI_RES, REL_HWHEEL_HI_RES],
        abs: vec![],
        props: vec![],
        rumble: false,
        msc_scan: false,
    };
    UinputDevice::create(&spec)
}
