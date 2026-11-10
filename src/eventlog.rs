//! A plain-text event log, like the original's "Open Event Log".

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());
const MAX_LOG_BYTES: u64 = 524288;

pub fn path() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("steamless-controller")
        .join("events.log")
}

fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        now.subsec_millis()
    )
}

pub fn write(msg: impl AsRef<str>) {
    let _g = LOCK.lock();
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // EventLog Write (0x1400276b0): append the line, flush, and if the file is
    // now over 512 KiB (ftell > 0x80000) rotate it (0x1400274e0): it moves to
    // events.old.log, replacing the previous one, and the next line starts fresh.
    let size = match std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        Ok(mut f) => {
            let _ = writeln!(f, "{}  {}", timestamp(), msg.as_ref());
            let _ = f.flush();
            f.metadata().map(|m| m.len()).unwrap_or(0)
        }
        Err(_) => return,
    };
    if size > MAX_LOG_BYTES {
        let _ = std::fs::rename(&p, p.with_file_name("events.old.log"));
    }
}
