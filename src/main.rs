//! Steamless Controller for Linux: use the 2026 Steam Controller as a normal
//! gamepad, keyboard and mouse without Steam.

mod app;
mod config;
mod controller;
mod desktop;
mod eventlog;
mod games;
mod hid;
mod mapping;
mod protocol;
mod retroarch;
mod tray;
mod uinput;

use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// Where finished builds are published (GitHub Releases of the Linux fork).
pub const RELEASES_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

const HELP: &str = concat!("Steamless Controller ", env!("CARGO_PKG_VERSION"), "\n",
env!("CARGO_PKG_REPOSITORY"), "\n\n\
Options:\n  \
--hidden              start in the system tray without opening the window\n  \
--page customize      open on Customize Controls\n  \
--screenshot <file>   draw the window, save it as a PNG and quit (never takes the controller)\n  \
--version             show the version\n  \
--help                show this text\n");

struct Args {
    hidden: bool,
    page: app::Page,
    screenshot: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut a = Args { hidden: false, page: app::Page::Controller, screenshot: None };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--hidden" => a.hidden = true,
            "--page" => {
                if it.next().as_deref() == Some("customize") {
                    a.page = app::Page::Customize;
                }
            }
            "--screenshot" => a.screenshot = it.next().map(PathBuf::from),
            "--version" | "-V" => {
                println!("steamless-controller {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--help" | "-h" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            other => eprintln!("unknown option {other} (see --help)"),
        }
    }
    a
}

/// One copy at a time: a second start just tells the first to show its window.
fn single_instance(shared: &Arc<controller::Shared>) -> Option<std::fs::File> {
    let dir = eventlog::path().parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("instance.lock")).ok()?;
    let ok = unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    let show_flag = dir.join("show-window");
    if !ok {
        let _ = std::fs::write(&show_flag, b"1");
        eprintln!("Steamless Controller is already running; asking it to show its window.");
        std::process::exit(0);
    }
    let _ = std::fs::remove_file(&show_flag);
    let shared = shared.clone();
    std::thread::spawn(move || {
        while !shared.quit.load(Ordering::SeqCst) {
            if show_flag.exists() {
                let _ = std::fs::remove_file(&show_flag);
                shared.show_window.store(true, Ordering::SeqCst);
                shared.request_repaint();
            }
            std::thread::sleep(Duration::from_millis(400));
        }
    });
    Some(lock)
}

fn run_window(shared: &Arc<controller::Shared>, tray: bool, page: app::Page, screenshot: Option<PathBuf>) {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Steamless Controller")
        .with_app_id("steamless-controller")
        .with_inner_size([760.0, 668.0])
        .with_min_inner_size([560.0, 440.0]);
    if screenshot.is_some() {
        // The self-test must not steal focus from whatever the person is doing.
        viewport = viewport.with_active(false);
    }
    let options = eframe::NativeOptions { viewport, run_and_return: true, ..Default::default() };
    let job = screenshot.map(|path| app::ScreenshotJob { path, frames: 0, requested: false });
    let sh = shared.clone();
    let res = eframe::run_native(
        "Steamless Controller",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, sh, tray, page, job)))),
    );
    *shared.repaint.lock().unwrap() = None;
    if let Err(e) = res {
        eprintln!("could not open the window: {e}");
        eventlog::write(format!("WINDOW: failed to open: {e}"));
    }
}

fn main() {
    let args = parse_args();
    let settings = config::Settings::load();
    let shared = controller::Shared::new(settings);
    let screenshot = args.screenshot.is_some();
    if screenshot {
        shared.passive.store(true, Ordering::SeqCst);
    }
    let _lock = if screenshot { None } else { single_instance(&shared) };
    let engine = controller::spawn_engine(shared.clone());
    let tray = !screenshot && tray::spawn(shared.clone());

    let mut show = !(args.hidden && tray);
    let mut page = args.page;
    let mut first = true;
    loop {
        if show {
            shared.show_window.store(false, Ordering::SeqCst);
            run_window(&shared, tray, page, if first { args.screenshot.clone() } else { None });
            first = false;
            if !tray || shared.quit.load(Ordering::SeqCst) {
                break;
            }
        }
        // Waiting in the tray until someone asks for the window again.
        while !shared.show_window.load(Ordering::SeqCst) && !shared.quit.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
        }
        if shared.quit.load(Ordering::SeqCst) {
            break;
        }
        show = true;
        page = app::Page::Customize;
    }
    shared.quit.store(true, Ordering::SeqCst);
    shared.send(controller::Command::Quit);
    let _ = engine.join();
}
