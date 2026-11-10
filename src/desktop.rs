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
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=Steamless Controller\n\
         Comment=Use the Steam Controller as a normal gamepad without Steam\n\
         Exec=\"{}\" --hidden\nIcon=input-gaming\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        exe.display()
    );
    std::fs::write(path, text)
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
