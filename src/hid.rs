//! Linux hidraw access: find the controller's vendor interfaces through sysfs,
//! read input reports, send feature and output reports.

use crate::protocol::{self, Transport};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HidInfo {
    pub dev_path: PathBuf,
    pub node: String,
    pub bus: u16,
    pub pid: u16,
    pub name: String,
    pub phys: String,
    pub serial: String,
}

impl HidInfo {
    pub fn transport(&self) -> Transport {
        Transport::from_ids(self.bus, self.pid)
    }
}

/// Top-level application collections in a report descriptor, as (usage page, usage).
pub fn top_level_collections(desc: &[u8]) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    let mut usage_page: u16 = 0;
    let mut last_usage: Option<(u16, u16)> = None;
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix == 0xFE {
            // Long item: [0xFE, size, tag, data...]
            let size = desc.get(i + 1).copied().unwrap_or(0) as usize;
            i += 3 + size;
            continue;
        }
        let size = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        let mut value: u32 = 0;
        for k in 0..size {
            value |= (desc.get(i + 1 + k).copied().unwrap_or(0) as u32) << (8 * k);
        }
        let tag = prefix & 0xFC;
        match tag {
            0x04 => usage_page = value as u16,
            0x08 => {
                if size == 4 {
                    last_usage = Some(((value >> 16) as u16, value as u16));
                } else {
                    last_usage = Some((usage_page, value as u16));
                }
            }
            0xA0 => {
                if depth == 0 && value == 0x01 {
                    if let Some(u) = last_usage {
                        out.push(u);
                    }
                }
                depth += 1;
                last_usage = None;
            }
            0xC0 => {
                depth -= 1;
            }
            // Main items other than collection clear local state.
            0x80 | 0x90 | 0xB0 => last_usage = None,
            _ => {}
        }
        i += 1 + size;
    }
    out
}

fn uevent_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
        .map(|s| s.to_string())
}

/// Every Steam Controller game-input interface (usage page 0xFF00, usage 1).
pub fn enumerate_controllers() -> Vec<HidInfo> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/hidraw") else {
        return out;
    };
    for entry in entries.flatten() {
        let node = entry.file_name().to_string_lossy().to_string();
        let dev = entry.path().join("device");
        let Ok(uevent) = fs::read_to_string(dev.join("uevent")) else {
            continue;
        };
        let Some(id) = uevent_value(&uevent, "HID_ID") else {
            continue;
        };
        let parts: Vec<&str> = id.split(':').collect();
        if parts.len() != 3 {
            continue;
        }
        let bus = u32::from_str_radix(parts[0], 16).unwrap_or(0) as u16;
        let vid = u32::from_str_radix(parts[1], 16).unwrap_or(0) as u16;
        let pid = u32::from_str_radix(parts[2], 16).unwrap_or(0) as u16;
        if vid != protocol::VALVE_VID || !protocol::CONTROLLER_PIDS.contains(&pid) {
            continue;
        }
        let Ok(desc) = fs::read(dev.join("report_descriptor")) else {
            continue;
        };
        let is_controller = top_level_collections(&desc)
            .iter()
            .any(|&(p, u)| p == protocol::VENDOR_USAGE_PAGE && u == protocol::CONTROLLER_USAGE);
        if !is_controller {
            continue;
        }
        out.push(HidInfo {
            dev_path: PathBuf::from(format!("/dev/{node}")),
            node,
            bus,
            pid,
            name: uevent_value(&uevent, "HID_NAME").unwrap_or_default(),
            phys: uevent_value(&uevent, "HID_PHYS").unwrap_or_default(),
            serial: uevent_value(&uevent, "HID_UNIQ").unwrap_or_default(),
        });
    }
    out.sort_by(|a, b| a.node.cmp(&b.node));
    out
}

/// Names of running processes that hold one of these hidraw nodes open
/// (best effort, reads /proc/*/fd).
pub fn holders(nodes: &[PathBuf]) -> Vec<String> {
    let mut names = Vec::new();
    let me = std::process::id();
    let Ok(procs) = fs::read_dir("/proc") else {
        return names;
    };
    for p in procs.flatten() {
        let Ok(pid) = p.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == me {
            continue;
        }
        let Ok(fds) = fs::read_dir(p.path().join("fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            if let Ok(target) = fs::read_link(fd.path()) {
                if nodes.iter().any(|n| *n == target) {
                    let comm = fs::read_to_string(p.path().join("comm")).unwrap_or_default();
                    let comm = comm.trim().to_string();
                    if !names.contains(&comm) {
                        names.push(comm);
                    }
                    break;
                }
            }
        }
    }
    names
}

pub struct HidDevice {
    file: File,
}

const fn hidiocsfeature(len: usize) -> libc::c_ulong {
    // _IOC(_IOC_WRITE|_IOC_READ, 'H', 0x06, len)
    ((3u64 << 30) | ((len as u64) << 16) | ((b'H' as u64) << 8) | 0x06) as libc::c_ulong
}

impl HidDevice {
    pub fn open(path: &PathBuf) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)?;
        Ok(Self { file })
    }

    pub fn raw_fd(&self) -> i32 {
        self.file.as_raw_fd()
    }

    /// Reads one report if one is waiting. Returns Ok(0) when none is.
    pub fn read_now(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.file.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(e),
        }
    }

    pub fn send_feature(&self, data: &[u8]) -> io::Result<()> {
        let mut buf = [0u8; protocol::FEATURE_LEN];
        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);
        let rc = unsafe {
            libc::ioctl(self.raw_fd(), hidiocsfeature(buf.len()), buf.as_mut_ptr())
        };
        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub fn write_output(&mut self, data: &[u8]) -> io::Result<()> {
        // Non-blocking descriptor: a busy link drops the write rather than
        // stalling the read loop, like the original's queued haptics.
        match self.file.write(data) {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vendor_collection() {
        // Dock interface descriptor from the puck on this PC.
        let hex = "0600ff0902a1018542150026ff0075089535094281028579150026ff0075089501097981028501953f0901b1028502953f0901b102c0";
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(top_level_collections(&bytes), vec![(0xFF00, 0x0002)]);
    }
}
