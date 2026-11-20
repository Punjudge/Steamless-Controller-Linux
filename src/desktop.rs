//! Small desktop integrations: start with login (XDG autostart), opening the
//! event log, and picking a program with the desktop's own file dialog.

use std::path::PathBuf;

fn autostart_file() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("autostart")
        .join("steamless-controller.desktop")
}

pub fn autostart_enabled() -> bool {
    std::fs::read_to_string(autostart_file())
        .map(|t| !t.lines().any(|l| l.trim() == "Hidden=true"))
        .unwrap_or(false)
}

pub fn set_autostart(on: bool) -> std::io::Result<()> {
    let path = autostart_file();
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let exe = std::env::current_exe()?;
    std::fs::write(path, autostart_text(&exe))
}

/// The login entry for the program at `exe`. It starts in the tray, as the
/// original's Run value does (0x140012fa0 writes the bare program path, and
/// its start-up 0x14000c9e0 never opens the window).
pub fn autostart_text(exe: &std::path::Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Steamless Controller\n\
         Comment=Use the Steam Controller as a normal gamepad without Steam\n\
         Exec=\"{}\" --hidden\nIcon=input-gaming\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    )
}

/// At every start the original writes its login entry again when start with
/// login is on (0x14000c9e0 calls 0x140012fa0 right after loading the
/// settings), so the entry follows the program when it is moved or installed
/// another way. Returns the new entry's program when it had to change.
pub fn sync_autostart() -> Option<PathBuf> {
    if !autostart_enabled() {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let want = autostart_text(&exe);
    let path = autostart_file();
    if std::fs::read_to_string(&path).ok().as_deref() == Some(want.as_str()) {
        return None;
    }
    std::fs::write(path, want).ok()?;
    Some(exe)
}

pub fn open_path(path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if !path.exists() {
        let _ = std::fs::write(path, "");
    }
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

/// Opens a web page in the person's browser.
pub fn open_url(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Asks the desktop for a program file (KDE's dialog, then GTK's). Blocks, so
/// call it from a thread.
pub fn pick_program() -> Option<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let tries: [(&str, Vec<String>); 2] = [
        ("kdialog", vec!["--title".into(), "Choose a program".into(), "--getopenfilename".into(), home.display().to_string()]),
        ("zenity", vec!["--file-selection".into(), "--title=Choose a program".into()]),
    ];
    for (prog, args) in tries {
        match std::process::Command::new(prog).args(&args).output() {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                return if out.status.success() && !s.is_empty() { Some(PathBuf::from(s)) } else { None };
            }
            Err(_) => continue,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_entry_starts_in_the_tray() {
        let t = autostart_text(std::path::Path::new("/usr/bin/steamless-controller"));
        assert!(t.lines().any(|l| l == "Exec=\"/usr/bin/steamless-controller\" --hidden"));
        assert!(!t.contains("Hidden=true"));
    }
}
