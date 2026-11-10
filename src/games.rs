//! Finding games and watching what runs: the Linux side of the original's
//! SteamWatcher, GameLibrary, LauncherGames and ForegroundWatcher.

use crate::config::{GameMatch, GameProfile};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum SteamState {
    NoSteam,
    Idle,
    InGame,
}

#[derive(Clone, Debug)]
pub struct ProcInfo {
    pub pid: u32,
    /// Start time in clock ticks since boot, for "newest wins".
    pub start: u64,
    /// Lower-case program file names this process answers to (comm, exe,
    /// argv[0], and a Windows .exe in the command line under Proton/Wine).
    pub names: Vec<String>,
    /// Steam app id from Steam's launch wrapper ("SteamLaunch AppId=N").
    pub steam_app: Option<u32>,
    pub display: String,
}

fn base_name(s: &str) -> String {
    let s = s.trim_matches('"');
    let s = s.rsplit(['/', '\\']).next().unwrap_or(s);
    s.to_lowercase()
}

pub fn scan_processes() -> Vec<ProcInfo> {
    let mut out = Vec::new();
    let me = std::process::id();
    let Ok(entries) = fs::read_dir("/proc") else {
        return out;
    };
    for e in entries.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == me {
            continue;
        }
        let dir = e.path();
        let Ok(cmdline) = fs::read(dir.join("cmdline")) else {
            continue;
        };
        if cmdline.is_empty() {
            continue; // kernel threads
        }
        let args: Vec<String> = cmdline
            .split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).to_string())
            .collect();
        let comm = fs::read_to_string(dir.join("comm")).unwrap_or_default().trim().to_string();
        let start = fs::read_to_string(dir.join("stat"))
            .ok()
            .and_then(|s| {
                let after = s.rsplit_once(')')?.1.to_string();
                after.split_whitespace().nth(19)?.parse::<u64>().ok()
            })
            .unwrap_or(0);
        let mut names = Vec::new();
        let mut push = |n: String| {
            if !n.is_empty() && !names.contains(&n) {
                names.push(n);
            }
        };
        push(comm.to_lowercase());
        if let Ok(exe) = fs::read_link(dir.join("exe")) {
            push(base_name(&exe.to_string_lossy()));
        }
        if let Some(a0) = args.first() {
            push(base_name(a0));
        }
        for a in &args {
            if a.to_lowercase().ends_with(".exe") {
                push(base_name(a));
            }
        }
        let mut steam_app = None;
        for (i, a) in args.iter().enumerate() {
            if let Some(id) = a.strip_prefix("AppId=") {
                if i > 0 && args[..i].iter().any(|x| x == "SteamLaunch") {
                    steam_app = id.parse::<u32>().ok().filter(|v| *v != 0);
                }
            }
        }
        let display = args
            .iter()
            .find(|a| a.to_lowercase().ends_with(".exe"))
            .map(|a| base_name(a))
            .unwrap_or_else(|| comm.clone());
        out.push(ProcInfo { pid, start, names, steam_app, display });
    }
    out
}

pub fn steam_state(procs: &[ProcInfo]) -> SteamState {
    let steam = procs.iter().any(|p| p.names.iter().any(|n| n == "steam" || n == "steam.sh"));
    if !steam {
        return SteamState::NoSteam;
    }
    if procs.iter().any(|p| p.steam_app.is_some()) {
        SteamState::InGame
    } else {
        SteamState::Idle
    }
}

/// The key of the game profile that is running, newest process first.
pub fn active_profile(procs: &[ProcInfo], games: &[GameProfile]) -> Option<String> {
    let mut best: Option<(u64, String)> = None;
    for p in procs {
        for g in games {
            let hit = match &g.matcher {
                GameMatch::Steam(id) => p.steam_app == Some(*id),
                GameMatch::Program(name) => {
                    let n = name.to_lowercase();
                    p.names.iter().any(|x| *x == n)
                }
            };
            if hit && best.as_ref().map(|b| p.start >= b.0).unwrap_or(true) {
                best = Some((p.start, g.matcher.key()));
            }
        }
    }
    best.map(|b| b.1)
}

/// Programs running now that look like something a person launched, for the
/// "Add an app that's running now" list.
pub fn running_apps(procs: &[ProcInfo]) -> Vec<(String, GameMatch)> {
    const SKIP: &[&str] = &[
        "systemd", "bash", "zsh", "fish", "sh", "sudo", "kworker", "pipewire", "wireplumber",
        "dbus-daemon", "dbus-broker", "kwin_wayland", "plasmashell", "xdg-desktop-portal",
        "steamwebhelper", "reaper", "pressure-vessel", "wineserver", "wine64-preloader",
        "steam-runtime", "services.exe", "winedevice.exe", "explorer.exe", "plugplay.exe",
        "rpcss.exe", "svchost.exe", "tabtip.exe", "conhost.exe", "start.exe",
        "steamless-controller", "Xwayland", "xwayland", "kded6", "ksmserver", "baloo_file",
        "kactivitymanagerd", "polkitd", "gvfsd", "at-spi-bus-launcher", "at-spi2-registryd",
    ];
    let mut seen = BTreeMap::new();
    for p in procs {
        if let Some(id) = p.steam_app {
            seen.insert(format!("Steam game {id}"), GameMatch::Steam(id));
            continue;
        }
        let d = p.display.clone();
        if d.is_empty() || SKIP.iter().any(|s| s.eq_ignore_ascii_case(&d)) || d.starts_with("kworker") {
            continue;
        }
        seen.entry(d.clone()).or_insert(GameMatch::Program(d));
    }
    seen.into_iter().collect()
}

#[derive(Clone, Debug)]
pub struct LibraryEntry {
    pub name: String,
    pub source: &'static str,
    pub matcher: GameMatch,
}

fn steam_roots() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let mut roots = vec![
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];
    roots.dedup();
    roots.into_iter().filter(|r| r.join("steamapps").exists()).collect()
}

fn vdf_values<'a>(text: &'a str, key: &str) -> Vec<&'a str> {
    let needle = format!("\"{key}\"");
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix(&needle)?.trim();
            rest.strip_prefix('"')?.strip_suffix('"')
        })
        .collect()
}

pub fn steam_games() -> Vec<LibraryEntry> {
    let mut libs: Vec<PathBuf> = Vec::new();
    for root in steam_roots() {
        libs.push(root.join("steamapps"));
        if let Ok(t) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            for p in vdf_values(&t, "path") {
                libs.push(Path::new(&p.replace("\\\\", "\\")).join("steamapps"));
            }
        }
    }
    libs.sort();
    libs.dedup();
    let mut out: BTreeMap<u32, LibraryEntry> = BTreeMap::new();
    for lib in libs {
        let Ok(entries) = fs::read_dir(&lib) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(t) = fs::read_to_string(e.path()) else {
                continue;
            };
            let Some(id) = vdf_values(&t, "appid").first().and_then(|v| v.parse::<u32>().ok()) else {
                continue;
            };
            let title = vdf_values(&t, "name").first().map(|s| s.to_string()).unwrap_or_default();
            // Runtimes and Proton itself are not games.
            let lower = title.to_lowercase();
            if lower.starts_with("proton") || lower.contains("steam linux runtime") || lower.contains("steamworks common") {
                continue;
            }
            out.insert(id, LibraryEntry { name: title, source: "Steam", matcher: GameMatch::Steam(id) });
        }
    }
    out.into_values().collect()
}

/// Desktop applications (the Linux equivalent of Start Menu shortcuts).
pub fn desktop_apps() -> Vec<LibraryEntry> {
    let home = dirs::home_dir().unwrap_or_default();
    let dirs_ = [
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        home.join(".local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        home.join(".local/share/flatpak/exports/share/applications"),
    ];
    let mut out: BTreeMap<String, LibraryEntry> = BTreeMap::new();
    for d in dirs_ {
        let Ok(entries) = fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("desktop") {
                continue;
            }
            let Ok(t) = fs::read_to_string(&p) else {
                continue;
            };
            let mut in_entry = false;
            let (mut name, mut exec, mut hidden, mut is_game) = (String::new(), String::new(), false, false);
            for l in t.lines() {
                let l = l.trim();
                if l.starts_with('[') {
                    in_entry = l == "[Desktop Entry]";
                    continue;
                }
                if !in_entry {
                    continue;
                }
                if let Some(v) = l.strip_prefix("Name=") {
                    if name.is_empty() {
                        name = v.to_string();
                    }
                } else if let Some(v) = l.strip_prefix("Exec=") {
                    exec = v.to_string();
                } else if l == "NoDisplay=true" || l == "Hidden=true" {
                    hidden = true;
                } else if let Some(v) = l.strip_prefix("Categories=") {
                    is_game = v.split(';').any(|c| c == "Game");
                }
            }
            if hidden || name.is_empty() || exec.is_empty() {
                continue;
            }
            // Steam's own shortcuts are covered by the Steam list.
            if exec.contains("steam://rungameid/") {
                continue;
            }
            let prog = exec
                .split_whitespace()
                .find(|w| !w.contains('=') && *w != "env")
                .map(base_name)
                .unwrap_or_default();
            if prog.is_empty() || prog == "flatpak" && !exec.contains("--command") && !is_game {
                continue;
            }
            let prog = if prog == "flatpak" {
                exec.split_whitespace().last().map(base_name).unwrap_or(prog)
            } else {
                prog
            };
            out.entry(name.to_lowercase()).or_insert(LibraryEntry {
                name,
                source: if is_game { "Games" } else { "Applications" },
                matcher: GameMatch::Program(prog),
            });
        }
    }
    let mut v: Vec<LibraryEntry> = out.into_values().collect();
    v.sort_by_key(|e| (e.source != "Games", e.name.to_lowercase()));
    v
}

pub fn library() -> Vec<LibraryEntry> {
    let mut v = steam_games();
    v.extend(desktop_apps());
    v
}
