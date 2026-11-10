//! The window: controller status and app settings, and "Customize Controls"
//! (the original's remap window) with the same sections and rules.

use crate::config::*;
use crate::controller::{Command, Shared};
use crate::desktop;
use crate::eventlog;
use crate::games::{self, LibraryEntry, SteamState};
use crate::mapping::{apply_stick, trigger_raw_at};
use egui::{Align, Color32, CornerRadius, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Colours of the original's settings window.
const BG_TOP: Color32 = Color32::from_rgb(0x1b, 0x28, 0x38);
const BG_BOTTOM: Color32 = Color32::from_rgb(0x15, 0x20, 0x2c);
const TITLEBAR: Color32 = Color32::from_rgb(0x17, 0x1a, 0x21);
const ACCENT: Color32 = Color32::from_rgb(0x66, 0xc0, 0xf4);
const TEXT: Color32 = Color32::from_rgb(0xc7, 0xd5, 0xe0);
const DIM: Color32 = Color32::from_rgb(0x8f, 0x98, 0xa0);
const LABEL: Color32 = Color32::from_rgb(0xae, 0xb9, 0xc2);
const SELECT: Color32 = Color32::from_rgb(0x10, 0x19, 0x25);
const GREEN_TOP: Color32 = Color32::from_rgb(0x94, 0xc6, 0x3d);
const GREEN_BOTTOM: Color32 = Color32::from_rgb(0x5d, 0x8a, 0x1f);
const GREEN_TEXT: Color32 = Color32::from_rgb(0x13, 0x26, 0x0a);
const DANGER: Color32 = Color32::from_rgb(0xc0, 0x39, 0x2b);
const METER: Color32 = Color32::from_rgb(0x4a, 0x6a, 0x85);
const METER_HOT: Color32 = Color32::from_rgb(0x5b, 0xa3, 0x2b);
const STICK_RAW: Color32 = Color32::from_rgb(0x6b, 0x77, 0x85);
const STICK_OUT: Color32 = Color32::from_rgb(0x1f, 0x5f, 0xbf);

fn row_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 6)
}
fn row_border() -> Color32 {
    Color32::from_rgba_unmultiplied(255, 255, 255, 15)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Controller,
    Customize,
}

#[derive(Clone, PartialEq, Debug)]
enum Target {
    Default,
    Game(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Slot {
    L4,
    L5,
    R4,
    R5,
    PadClick(usize),
    PadTouch(usize),
    PadDir(usize, u8),
    TriggerFull(usize),
}

fn pad_mut(p: &mut Profile, side: usize) -> &mut PadSettings {
    if side == 0 { &mut p.left_pad } else { &mut p.right_pad }
}

fn slot_mut(p: &mut Profile, s: Slot) -> &mut Binding {
    match s {
        Slot::L4 => &mut p.back.l4,
        Slot::L5 => &mut p.back.l5,
        Slot::R4 => &mut p.back.r4,
        Slot::R5 => &mut p.back.r5,
        Slot::PadClick(side) => &mut pad_mut(p, side).click,
        Slot::PadTouch(side) => &mut pad_mut(p, side).touch,
        Slot::PadDir(side, d) => {
            let pad = pad_mut(p, side);
            match d {
                DIR_UP => &mut pad.up,
                DIR_DOWN => &mut pad.down,
                DIR_LEFT => &mut pad.left,
                _ => &mut pad.right,
            }
        }
        Slot::TriggerFull(side) => {
            if side == 0 { &mut p.left_trigger.full } else { &mut p.right_trigger.full }
        }
    }
}

enum Pending {
    Switch(Target),
    NewGame(GameProfile),
    Close,
    Page(Page),
}

pub struct ScreenshotJob {
    pub path: std::path::PathBuf,
    pub frames: u32,
    pub requested: bool,
}

pub struct App {
    shared: Arc<Shared>,
    tray: bool,
    page: Page,
    target: Target,
    /// A new game profile that hasn't been saved yet.
    new_game: Option<GameProfile>,
    draft: Profile,
    saved: Profile,
    search: String,
    library: Option<Vec<LibraryEntry>>,
    running: Vec<(String, GameMatch)>,
    listening: Option<Slot>,
    pending: Option<Pending>,
    autostart: bool,
    message: Option<(String, Instant)>,
    browse_rx: Option<Receiver<Option<std::path::PathBuf>>>,
    browse_tx: Sender<Option<std::path::PathBuf>>,
    allow_close: bool,
    screenshot: Option<ScreenshotJob>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, shared: Arc<Shared>, tray: bool, page: Page, screenshot: Option<ScreenshotJob>) -> Self {
        style(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        *shared.repaint.lock().unwrap() = Some(Box::new(move || ctx.request_repaint()));
        let saved = shared.settings.read().unwrap().default_profile.clone();
        let (browse_tx, browse_rx) = std::sync::mpsc::channel();
        let _ = browse_rx;
        Self {
            shared,
            tray,
            page,
            target: Target::Default,
            new_game: None,
            draft: saved.clone(),
            saved,
            search: String::new(),
            library: None,
            running: Vec::new(),
            listening: None,
            pending: None,
            autostart: desktop::autostart_enabled(),
            message: None,
            browse_rx: None,
            browse_tx,
            allow_close: false,
            screenshot,
        }
    }

    fn dirty(&self) -> bool {
        self.new_game.is_some() || self.draft != self.saved
    }

    fn ps(&self) -> bool {
        self.draft.platform == Platform::PlayStation
    }

    fn target_name(&self) -> String {
        match &self.target {
            Target::Default => "Default (all other games)".into(),
            Target::Game(key) => {
                if let Some(g) = &self.new_game {
                    if g.matcher.key() == *key {
                        return g.name.clone();
                    }
                }
                self.shared
                    .settings
                    .read()
                    .unwrap()
                    .games
                    .iter()
                    .find(|g| g.matcher.key() == *key)
                    .map(|g| g.name.clone())
                    .unwrap_or_else(|| key.clone())
            }
        }
    }

    fn load_target(&mut self, t: Target) {
        let s = self.shared.settings.read().unwrap();
        let prof = match &t {
            Target::Default => Some(s.default_profile.clone()),
            Target::Game(key) => s.games.iter().find(|g| g.matcher.key() == *key).map(|g| g.profile.clone()),
        };
        drop(s);
        if let Some(p) = prof {
            self.saved = p.clone();
            self.draft = p;
            self.target = t;
            self.new_game = None;
        }
        self.cancel_listen();
    }

    fn start_new_game(&mut self, g: GameProfile) {
        let exists = self.shared.settings.read().unwrap().games.iter().any(|x| x.matcher == g.matcher);
        if exists {
            self.load_target(Target::Game(g.matcher.key()));
            return;
        }
        self.target = Target::Game(g.matcher.key());
        self.saved = g.profile.clone();
        self.draft = g.profile.clone();
        self.new_game = Some(g);
        self.cancel_listen();
    }

    /// Runs `p` now, or asks first when there are unsaved changes.
    fn guard(&mut self, p: Pending) {
        if self.dirty() {
            self.pending = Some(p);
        } else {
            self.run_pending(p);
        }
    }

    fn run_pending(&mut self, p: Pending) {
        match p {
            Pending::Switch(t) => self.load_target(t),
            Pending::NewGame(g) => self.start_new_game(g),
            Pending::Page(pg) => {
                self.revert();
                self.page = pg;
            }
            Pending::Close => {
                self.revert();
                self.allow_close = true;
            }
        }
    }

    fn revert(&mut self) {
        if self.new_game.is_some() {
            self.load_target(Target::Default);
        } else {
            self.draft = self.saved.clone();
        }
        self.cancel_listen();
    }

    fn save(&mut self) {
        let mut p = self.draft.clone();
        sanitize_profile(&mut p);
        {
            let mut s = self.shared.settings.write().unwrap();
            match &self.target {
                Target::Default => s.default_profile = p.clone(),
                Target::Game(key) => {
                    if let Some(g) = s.games.iter_mut().find(|g| g.matcher.key() == *key) {
                        g.profile = p.clone();
                    } else if let Some(mut g) = self.new_game.take() {
                        g.profile = p.clone();
                        s.games.push(g);
                    }
                }
            }
            s.save();
        }
        self.new_game = None;
        self.saved = p.clone();
        self.draft = p;
        self.shared.send(Command::SettingsChanged);
        eventlog::write(format!("REMAP: saved profile '{}'", self.target_name()));
        self.flash("Saved.");
    }

    fn delete_profile(&mut self) {
        if let Target::Game(key) = self.target.clone() {
            if self.new_game.is_none() {
                let mut s = self.shared.settings.write().unwrap();
                s.games.retain(|g| g.matcher.key() != key);
                s.save();
                eventlog::write(format!("REMAP: deleted profile {key}"));
            }
            self.new_game = None;
            self.load_target(Target::Default);
            self.flash("Profile deleted.");
        }
    }

    fn flash(&mut self, m: &str) {
        self.message = Some((m.to_string(), Instant::now()));
    }

    fn listen(&mut self, s: Slot) {
        self.listening = Some(s);
        *self.shared.captured.lock().unwrap() = None;
        self.shared.capturing.store(true, Ordering::SeqCst);
    }

    fn cancel_listen(&mut self) {
        self.listening = None;
        self.shared.capturing.store(false, Ordering::SeqCst);
    }

    fn poll_listen(&mut self, ctx: &egui::Context) {
        let Some(slot) = self.listening else {
            return;
        };
        let captured = self.shared.captured.lock().unwrap().take();
        if let Some(b) = captured {
            *slot_mut(&mut self.draft, slot) = b;
            self.cancel_listen();
            return;
        }
        // A key on the keyboard binds that key (with held modifiers).
        let hit = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Key { key, pressed: true, modifiers, .. } => linux_key(*key).map(|c| (c, *modifiers)),
                _ => None,
            })
        });
        if let Some((code, m)) = hit {
            let mut mods = 0;
            if m.ctrl {
                mods |= MOD_CTRL;
            }
            if m.alt {
                mods |= MOD_ALT;
            }
            if m.shift {
                mods |= MOD_SHIFT;
            }
            *slot_mut(&mut self.draft, slot) = Binding::Key { code, mods };
            self.cancel_listen();
        }
    }
}

fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = BG_TOP;
    v.window_fill = Color32::from_rgb(0x1e, 0x2a, 0x38);
    v.extreme_bg_color = SELECT;
    v.selection.bg_fill = Color32::from_rgb(0x2a, 0x47, 0x5e);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.widgets.inactive.bg_fill = SELECT;
    v.widgets.inactive.weak_bg_fill = SELECT;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x2c, 0x3b, 0x4b));
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x1a, 0x2a, 0x3c);
    v.widgets.hovered.bg_fill = Color32::from_rgb(0x1a, 0x2a, 0x3c);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0x23, 0x36, 0x4a);
    v.widgets.active.bg_fill = Color32::from_rgb(0x23, 0x36, 0x4a);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(4);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, row_border());
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 4.0);
        s.spacing.slider_width = 180.0;
    });
}

fn paint_background(ui: &egui::Ui) {
    let rect = ui.max_rect();
    let mut mesh = egui::Mesh::default();
    let i = mesh.vertices.len() as u32;
    mesh.colored_vertex(rect.left_top(), BG_TOP);
    mesh.colored_vertex(rect.right_top(), BG_TOP);
    mesh.colored_vertex(rect.right_bottom(), BG_BOTTOM);
    mesh.colored_vertex(rect.left_bottom(), BG_BOTTOM);
    mesh.add_triangle(i, i + 1, i + 2);
    mesh.add_triangle(i, i + 2, i + 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(text.to_uppercase()).size(12.0).strong().color(ACCENT));
    ui.add_space(2.0);
}

fn card<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(row_fill())
        .stroke(Stroke::new(1.0, row_border()))
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.set_min_height(26.0);
        ui.label(RichText::new(label).color(LABEL));
        ui.with_layout(Layout::right_to_left(Align::Center), add);
    });
}

fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(DIM));
}

fn choice<T: PartialEq + Copy>(ui: &mut egui::Ui, id: &str, value: &mut T, options: &[(T, &str)]) {
    let current = options.iter().find(|(v, _)| v == value).map(|(_, l)| *l).unwrap_or("");
    egui::ComboBox::from_id_salt(id).width(230.0).selected_text(current).show_ui(ui, |ui| {
        for (v, l) in options {
            ui.selectable_value(value, *v, *l);
        }
    });
}

fn green_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    let b = egui::Button::new(RichText::new(text).strong().color(if enabled { GREEN_TEXT } else { DIM }))
        .fill(if enabled { GREEN_BOTTOM.lerp_to_gamma(GREEN_TOP, 0.5) } else { SELECT })
        .min_size(Vec2::new(96.0, 30.0));
    ui.add_enabled(enabled, b)
}

/// Linux key code for a key pressed in the window.
fn linux_key(k: egui::Key) -> Option<u16> {
    use egui::Key::*;
    let letters = [
        (A, 30), (B, 48), (C, 46), (D, 32), (E, 18), (F, 33), (G, 34), (H, 35), (I, 23), (J, 36), (K, 37),
        (L, 38), (M, 50), (N, 49), (O, 24), (P, 25), (Q, 16), (R, 19), (S, 31), (T, 20), (U, 22), (V, 47),
        (W, 17), (X, 45), (Y, 21), (Z, 44),
        (Num0, 11), (Num1, 2), (Num2, 3), (Num3, 4), (Num4, 5), (Num5, 6), (Num6, 7), (Num7, 8), (Num8, 9), (Num9, 10),
        (F1, 59), (F2, 60), (F3, 61), (F4, 62), (F5, 63), (F6, 64), (F7, 65), (F8, 66), (F9, 67), (F10, 68),
        (F11, 87), (F12, 88),
        (Escape, 1), (Tab, 15), (Space, 57), (Enter, 28), (Backspace, 14),
        (ArrowUp, 103), (ArrowDown, 108), (ArrowLeft, 105), (ArrowRight, 106),
        (Home, 102), (End, 107), (PageUp, 104), (PageDown, 109), (Insert, 110), (Delete, 111),
        (Minus, 12), (Equals, 13), (OpenBracket, 26), (CloseBracket, 27), (Semicolon, 39), (Quote, 40),
        (Backtick, 41), (Backslash, 43), (Comma, 51), (Period, 52), (Slash, 53),
    ];
    letters.iter().find(|(e, _)| *e == k).map(|(_, c)| *c)
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.shared.quit.load(Ordering::SeqCst) {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.dirty() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.pending = Some(Pending::Close);
            } else {
                self.cancel_listen();
            }
        }
        if self.allow_close && self.pending.is_none() {
            self.allow_close = false;
            self.cancel_listen();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.shared.show_window.swap(false, Ordering::SeqCst) {
            self.page = Page::Customize;
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.poll_listen(ctx);
        if let Some(rx) = &self.browse_rx {
            if let Ok(res) = rx.try_recv() {
                self.browse_rx = None;
                if let Some(path) = res {
                    let file = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
                    if !file.is_empty() {
                        let mut prof = self.shared.settings.read().unwrap().default_profile.clone();
                        prof.use_default_mappings = false;
                        let name = path.file_stem().map(|f| f.to_string_lossy().to_string()).unwrap_or(file.clone());
                        self.guard(Pending::NewGame(GameProfile { name, matcher: GameMatch::Program(file), profile: prof }));
                    }
                }
            }
        }
        // Screenshot self-test: draw, then save the window as a PNG.
        if let Some(job) = &mut self.screenshot {
            ctx.request_repaint();
            job.frames += 1;
            if job.frames >= 120 && !job.requested {
                job.requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            let shot = ctx.input(|i| {
                i.raw.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                let [w, h] = img.size;
                let mut bytes = Vec::with_capacity(w * h * 4);
                for p in &img.pixels {
                    bytes.extend_from_slice(&p.to_srgba_unmultiplied());
                }
                if let Some(dir) = job.path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                match image::RgbaImage::from_raw(w as u32, h as u32, bytes).map(|i| i.save(&job.path)) {
                    Some(Ok(())) => println!("saved {}", job.path.display()),
                    other => eprintln!("could not save the screenshot: {other:?}"),
                }
                self.shared.quit.store(true, Ordering::SeqCst);
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        } else if self.page == Page::Customize || self.shared.status().game_mode {
            ctx.request_repaint_after(Duration::from_millis(50));
        } else {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        paint_background(ui);
        egui::Panel::top("titlebar")
            .frame(egui::Frame::new().fill(TITLEBAR).inner_margin(egui::Margin::symmetric(14, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Steamless Controller").size(16.0).strong().color(Color32::WHITE));
                    ui.add_space(16.0);
                    for (pg, label) in [(Page::Controller, "Controller"), (Page::Customize, "Customize Controls")] {
                        let sel = self.page == pg;
                        let text = RichText::new(label).color(if sel { ACCENT } else { DIM }).strong();
                        if ui.add(egui::Button::new(text).frame(false)).clicked() && !sel {
                            if pg == Page::Controller {
                                self.guard(Pending::Page(pg));
                            } else {
                                self.page = pg;
                            }
                        }
                    }
                });
            });
        if self.page == Page::Customize {
            egui::Panel::bottom("actions")
                .frame(egui::Frame::new().fill(TITLEBAR).inner_margin(egui::Margin::symmetric(14, 10)))
                .show(ui, |ui| self.actions(ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(18, 8)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    // Keep the original's narrow column, centred in a wide window.
                    const COLUMN: f32 = 760.0;
                    let pad = ((ui.available_width() - COLUMN) / 2.0).max(0.0);
                    ui.horizontal_top(|ui| {
                        ui.add_space(pad);
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width().min(COLUMN));
                            match self.page {
                                Page::Controller => self.controller_page(ui),
                                Page::Customize => self.customize_page(ui),
                            }
                        });
                    });
                });
            });
        self.unsaved_modal(ui.ctx());
    }
}

// ---------------------------------------------------------------- Controller page

impl App {
    fn controller_page(&mut self, ui: &mut egui::Ui) {
        let st = self.shared.status();
        let battery = self.shared.live.battery.load(Ordering::Relaxed);
        heading(ui, "Controller");
        card(ui, |ui| {
            let (title, colour) = if st.game_mode {
                ("Steamless Mode ON", METER_HOT)
            } else if st.connected {
                ("Connected (Steamless Mode OFF)", ACCENT)
            } else {
                ("No controller found", DIM)
            };
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 5.0, colour);
                ui.label(RichText::new(title).size(18.0).strong().color(Color32::WHITE));
            });
            ui.add_space(4.0);
            let link = st.transport.map(|t| t.label()).unwrap_or("—");
            row(ui, "Connection", |ui| {
                ui.label(link);
            });
            row(ui, "Battery", |ui| {
                ui.label(if battery >= 0 { format!("{battery}%") } else { "Unknown until Steamless Mode is on".into() });
            });
            row(ui, "Shows up as", |ui| {
                ui.label(match st.platform {
                    Some(Platform::PlayStation) => "PlayStation-style controller",
                    _ => "Xbox 360-style controller",
                });
            });
            row(ui, "Steam", |ui| {
                ui.label(match st.steam {
                    Some(SteamState::InGame) => "Running, in a game",
                    Some(SteamState::Idle) => "Running",
                    _ => "Not running",
                });
            });
            if let Some(g) = &st.active_game {
                row(ui, "Game profile in use", |ui| {
                    ui.label(RichText::new(g).color(ACCENT));
                });
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let on = st.want_control;
                let label = if on { "Turn Steamless Mode off" } else { "Turn Steamless Mode on" };
                if green_button(ui, label, true).clicked() {
                    self.shared.send(Command::ToggleManual);
                }
                if self.shared.settings.read().unwrap().auto_mode != AutoMode::Manual {
                    hint(ui, "Turning it on or off by hand switches Control Mode to Manual.");
                }
            });
        });

        heading(ui, "Control Mode");
        card(ui, |ui| {
            let mut mode = self.shared.settings.read().unwrap().auto_mode;
            let before = mode;
            for m in AutoMode::ALL {
                ui.radio_value(&mut mode, m, m.label());
            }
            if mode != before {
                self.shared.send(Command::SetAutoMode(mode));
            }
            hint(ui, match mode {
                AutoMode::Manual => "Steamless Mode stays the way you set it.",
                AutoMode::OffWhileSteam => "Steamless Mode is on only while Steam is closed.",
                AutoMode::OffOnlyInGame => "Steamless Mode steps aside while you play a Steam game.",
                AutoMode::OffUnlessProfile => "Steamless Mode is on only while a game with its own profile is running.",
            });
        });

        heading(ui, "Emulators (RetroArch)");
        card(ui, |ui| {
            hint(ui, "Turn Steamless Mode on, then pick the joypad driver in RetroArch under Settings → Drivers → Joypad. In Xbox mode RetroArch sets the buttons up by itself (it sees a \"Microsoft X-Box 360 pad\").");
            ui.add_space(2.0);
            for (driver, works, text) in [
                ("udev", true, "Works, with rumble. Recommended."),
                ("sdl2", true, "Works, with rumble."),
                ("linuxraw", true, "Works, buttons and sticks only (no rumble)."),
                ("parport", false, "Not for this controller (old parallel-port pads)."),
                ("test", false, "Not a real driver; for RetroArch's own testing."),
            ] {
                row(ui, driver, |ui| {
                    ui.label(RichText::new(text).color(if works { METER_HOT } else { DIM }));
                });
            }
            if st.platform == Some(Platform::PlayStation) {
                hint(ui, "PlayStation mode also works with udev and sdl2, but Xbox mode is the simplest choice for RetroArch.");
            }
            ui.add_space(4.0);
            if !crate::retroarch::found() {
                hint(ui, "RetroArch's settings weren't found. Start RetroArch once, then come back here.");
            } else {
                let installed = crate::retroarch::installed();
                row(ui, "Button setup in RetroArch", |ui| {
                    let label = if installed { "Set up again" } else { "Set up RetroArch for this controller" };
                    if green_button(ui, label, true).clicked() {
                        match crate::retroarch::install() {
                            Ok(_) => self.flash("RetroArch is set up. Restart RetroArch to use it."),
                            Err(e) => self.flash(&format!("Could not set up RetroArch: {e}")),
                        }
                    }
                    if installed {
                        ui.label(RichText::new("Done").color(METER_HOT));
                    }
                });
                hint(ui, "If RetroArch says the controller is \"not configured\", press the button and restart RetroArch. It works with udev and linuxraw; it doesn't change anything else in RetroArch.");
                if let Some((m, t)) = &self.message {
                    if t.elapsed() < Duration::from_secs(4) {
                        ui.label(RichText::new(m).color(ACCENT));
                    }
                }
            }
        });

        heading(ui, "App Settings");
        card(ui, |ui| {
            let mut auto = self.autostart;
            if ui.checkbox(&mut auto, "Start with login").changed() {
                match desktop::set_autostart(auto) {
                    Ok(()) => self.autostart = auto,
                    Err(e) => self.flash(&format!("Could not change start with login: {e}")),
                }
            }
            let (mut eol, mut notes) = {
                let s = self.shared.settings.read().unwrap();
                (s.enable_on_launch, s.notifications)
            };
            let a = ui.checkbox(&mut eol, "Enable on launch (Manual mode)").changed();
            let b = ui.checkbox(&mut notes, "Show notifications").changed();
            if a || b {
                let mut s = self.shared.settings.write().unwrap();
                s.enable_on_launch = eol;
                s.notifications = notes;
                s.save();
            }
            ui.horizontal(|ui| {
                if ui.button("Open Event Log").clicked() {
                    desktop::open_path(&eventlog::path());
                }
                if !self.tray {
                    hint(ui, "No system tray was found, so closing this window quits the app.");
                } else {
                    hint(ui, "Closing this window keeps the app running in the system tray.");
                }
            });
            if let Some((m, t)) = &self.message {
                if t.elapsed() < Duration::from_secs(4) {
                    ui.label(RichText::new(m).color(ACCENT));
                }
            }
            ui.horizontal(|ui| {
                hint(ui, &format!("Version {}", env!("CARGO_PKG_VERSION")));
                if ui.link("Downloads and updates").clicked() {
                    desktop::open_url(crate::RELEASES_URL);
                }
                if ui.link("Source code").clicked() {
                    desktop::open_url(env!("CARGO_PKG_REPOSITORY"));
                }
            });
        });

        let problems = st.access_error.is_some() || st.pad_error.is_some();
        let steam_holds = st.others_holding.iter().any(|h| h.contains("steam"));
        if problems || (steam_holds && st.game_mode) {
            heading(ui, "Setup");
            card(ui, |ui| {
                if let Some(e) = &st.access_error {
                    ui.label(RichText::new("The controller could not be opened.").color(DANGER).strong());
                    hint(ui, e);
                    hint(ui, "Steam's device rules (package steam-devices or steam) give your login access to the controller. Install them and plug the controller or puck in again.");
                }
                if let Some(e) = &st.pad_error {
                    ui.label(RichText::new("The virtual controller could not be created.").color(DANGER).strong());
                    hint(ui, e);
                    hint(ui, "Your login needs access to /dev/uinput (the input group, or Steam's device rules). Log out and in again after adding it.");
                }
                if steam_holds && st.game_mode {
                    ui.label(RichText::new("Steam is reading the controller too.").color(ACCENT));
                    hint(ui, "Games may see two controllers. Use a Control Mode that steps aside for Steam, or turn off Steam Input for this controller in Steam's settings.");
                }
            });
        }
        ui.add_space(12.0);
    }
}

// ---------------------------------------------------------------- Customize page

impl App {
    fn customize_page(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("Customize Controls").size(20.0).strong().color(Color32::WHITE));
        hint(ui, "Pick what each part of the controller does. Settings for a game apply while that game is running; everything else uses the default.");

        heading(ui, "Apply To");
        card(ui, |ui| self.apply_to(ui));

        let is_game = self.target != Target::Default;
        if is_game {
            heading(ui, "Mappings");
            card(ui, |ui| {
                ui.checkbox(&mut self.draft.use_default_mappings, "Use my default mappings");
                hint(ui, "When on, this game uses the default profile and nothing below applies.");
            });
        }
        let enabled = !(is_game && self.draft.use_default_mappings);
        ui.add_enabled_ui(enabled, |ui| {
            heading(ui, "General Settings");
            card(ui, |ui| {
                row(ui, "Controller Platform", |ui| {
                    let before = self.draft.platform;
                    choice(ui, "platform", &mut self.draft.platform, &[(Platform::Xbox, "Xbox"), (Platform::PlayStation, "PlayStation")]);
                    if before != self.draft.platform && self.draft.platform == Platform::Xbox {
                        for pad in [&mut self.draft.left_pad, &mut self.draft.right_pad] {
                            if pad.mode == PadMode::Ds4Touchpad {
                                pad.mode = PadMode::None;
                            }
                        }
                    }
                });
                if self.ps() {
                    hint(ui, "Shows up as a PlayStation-style controller with a touchpad. Motion controls are not sent.");
                }
                row(ui, "Rumble", |ui| {
                    ui.add(egui::Slider::new(&mut self.draft.rumble, 0..=100).suffix("%"));
                });
            });
            for side in [0usize, 1] {
                heading(ui, if side == 0 { "Left Trackpad" } else { "Right Trackpad" });
                card(ui, |ui| self.trackpad(ui, side));
            }
            heading(ui, "Left Grip");
            card(ui, |ui| {
                self.binding_row(ui, "L4 (upper)", Slot::L4);
                self.binding_row(ui, "L5 (lower)", Slot::L5);
            });
            heading(ui, "Right Grip");
            card(ui, |ui| {
                self.binding_row(ui, "R4 (upper)", Slot::R4);
                self.binding_row(ui, "R5 (lower)", Slot::R5);
            });
            for side in [0usize, 1] {
                heading(ui, if side == 0 { "Left Trigger" } else { "Right Trigger" });
                card(ui, |ui| self.trigger(ui, side));
            }
            for side in [0usize, 1] {
                heading(ui, if side == 0 { "Left Stick" } else { "Right Stick" });
                card(ui, |ui| self.stick(ui, side));
            }
        });
        ui.add_space(12.0);
    }

    fn apply_to(&mut self, ui: &mut egui::Ui) {
        let mut action: Option<Pending> = None;
        let mut browse = false;
        let current = self.target_name();
        let games: Vec<(String, String)> = self
            .shared
            .settings
            .read()
            .unwrap()
            .games
            .iter()
            .map(|g| (g.matcher.key(), g.name.clone()))
            .collect();
        let resp = egui::ComboBox::from_id_salt("apply_to")
            .width(ui.available_width() - 8.0)
            .height(420.0)
            .selected_text(RichText::new(&current).strong())
            .show_ui(ui, |ui| {
                if ui.selectable_label(self.target == Target::Default, "Default (all other games)").clicked() {
                    action = Some(Pending::Switch(Target::Default));
                }
                for (key, name) in &games {
                    let sel = self.target == Target::Game(key.clone());
                    if ui.selectable_label(sel, name).clicked() {
                        action = Some(Pending::Switch(Target::Game(key.clone())));
                    }
                }
                ui.separator();
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search games and apps…"));
                ui.menu_button("+ Add an app that's running now…", |ui| {
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        if self.running.is_empty() {
                            hint(ui, "Nothing found.");
                        }
                        for (name, m) in self.running.clone() {
                            if ui.button(&name).clicked() {
                                action = Some(Pending::NewGame(self.new_profile_for(name, m)));
                                ui.close();
                            }
                        }
                    });
                });
                if ui.button("+ Browse for a program…").clicked() {
                    browse = true;
                }
                ui.separator();
                let lib = self.library.get_or_insert_with(games::library).clone();
                let q = self.search.to_lowercase();
                let mut last_source = "";
                for e in lib.iter().filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q)).take(200) {
                    if e.source != last_source {
                        last_source = e.source;
                        ui.label(RichText::new(e.source).size(11.0).color(ACCENT));
                    }
                    if games.iter().any(|(k, _)| *k == e.matcher.key()) {
                        continue;
                    }
                    if ui.selectable_label(false, &e.name).clicked() {
                        action = Some(Pending::NewGame(self.new_profile_for(e.name.clone(), e.matcher.clone())));
                    }
                }
            });
        if resp.response.clicked() {
            self.running = games::running_apps(&games::scan_processes());
        }
        if browse && self.browse_rx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            self.browse_tx = tx.clone();
            self.browse_rx = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(desktop::pick_program());
            });
        }
        if let Some(a) = action {
            self.guard(a);
        }
        if let Target::Game(key) = &self.target {
            let how = if key.starts_with("steam:") { "Steam game" } else { "program" };
            hint(ui, &format!("Applies while this {how} is running ({}).", key.split_once(':').map(|x| x.1).unwrap_or(key)));
        }
        if self.new_game.is_some() {
            ui.label(RichText::new("New profile — press Save to keep it.").color(ACCENT));
        }
    }

    fn new_profile_for(&self, name: String, matcher: GameMatch) -> GameProfile {
        let mut profile = self.shared.settings.read().unwrap().default_profile.clone();
        profile.use_default_mappings = false;
        GameProfile { name, matcher, profile }
    }

    fn binding_row(&mut self, ui: &mut egui::Ui, label: &str, slot: Slot) {
        let ps = self.ps();
        let current = *slot_mut(&mut self.draft, slot);
        let listening = self.listening == Some(slot);
        row(ui, label, |ui| {
            if listening {
                if ui.button("Cancel").clicked() {
                    self.cancel_listen();
                }
                ui.label(RichText::new("Press a controller button or a key…").color(ACCENT));
                return;
            }
            if ui.button("Rebind").on_hover_text("Press a controller button or a keyboard key to bind it").clicked() {
                self.listen(slot);
            }
            let mut picked: Option<Binding> = None;
            egui::ComboBox::from_id_salt(("bind", format!("{slot:?}")))
                .width(200.0)
                .height(360.0)
                .selected_text(current.label(ps))
                .show_ui(ui, |ui| {
                    let mut item = |ui: &mut egui::Ui, b: Binding| {
                        if ui.selectable_label(current == b, b.label(ps)).clicked() {
                            picked = Some(b);
                        }
                    };
                    item(ui, Binding::NONE);
                    ui.label(RichText::new("Controller").size(11.0).color(ACCENT));
                    for a in Action::GAMEPAD {
                        item(ui, Binding::Action(a));
                    }
                    item(ui, Binding::Action(Action::Guide));
                    item(ui, Binding::Action(Action::TouchpadClick));
                    ui.label(RichText::new("Mouse").size(11.0).color(ACCENT));
                    item(ui, Binding::Action(Action::LeftMouse));
                    item(ui, Binding::Action(Action::RightMouse));
                    for m in [ExtraMouse::Middle, ExtraMouse::Back, ExtraMouse::Forward] {
                        item(ui, Binding::Mouse(m));
                    }
                    ui.label(RichText::new("Desktop").size(11.0).color(ACCENT));
                    item(ui, Binding::Action(Action::TouchKeyboard));
                    ui.label(RichText::new("Keyboard").size(11.0).color(ACCENT));
                    for (code, _) in KEYS {
                        item(ui, Binding::Key { code: *code, mods: 0 });
                    }
                });
            if let Some(b) = picked {
                *slot_mut(&mut self.draft, slot) = b;
            }
            badge(ui, &current, ps);
        });
    }

    fn trackpad(&mut self, ui: &mut egui::Ui, side: usize) {
        let ps = self.ps();
        let mut modes = vec![
            (PadMode::None, PadMode::None.label()),
            (PadMode::MousePointer, PadMode::MousePointer.label()),
            (PadMode::ScrollWheel, PadMode::ScrollWheel.label()),
        ];
        if ps {
            modes.push((PadMode::Ds4Touchpad, PadMode::Ds4Touchpad.label()));
        }
        modes.push((PadMode::DirectionalPad, PadMode::DirectionalPad.label()));
        modes.push((PadMode::SingleButton, PadMode::SingleButton.label()));
        let id = if side == 0 { "lpad" } else { "rpad" };
        let mode = {
            let pad = pad_mut(&mut self.draft, side);
            row(ui, "Movement Mode", |ui| choice(ui, &format!("{id}_mode"), &mut pad.mode, &modes));
            if pad.mode == PadMode::ScrollWheel {
                row(ui, "Scroll Direction", |ui| {
                    choice(ui, &format!("{id}_dir"), &mut pad.scroll_dir, &[
                        (ScrollDirection::Natural, "Natural"),
                        (ScrollDirection::Reversed, "Reversed"),
                    ])
                });
            }
            if matches!(pad.mode, PadMode::ScrollWheel | PadMode::MousePointer) {
                row(ui, "Speed", |ui| {
                    ui.add(egui::Slider::new(&mut pad.speed, SCROLL_SPEED_MIN..=SCROLL_SPEED_MAX).suffix("%"));
                });
            }
            pad.mode
        };
        match mode {
            PadMode::Ds4Touchpad => {
                hint(ui, "Touches and clicks go to the PlayStation touchpad.");
            }
            PadMode::DirectionalPad => {
                self.binding_row(ui, "Click (centre)", Slot::PadClick(side));
                self.binding_row(ui, "Touch / Tap", Slot::PadTouch(side));
                self.binding_row(ui, "Up", Slot::PadDir(side, DIR_UP));
                self.binding_row(ui, "Down", Slot::PadDir(side, DIR_DOWN));
                self.binding_row(ui, "Left", Slot::PadDir(side, DIR_LEFT));
                self.binding_row(ui, "Right", Slot::PadDir(side, DIR_RIGHT));
                let pad = pad_mut(&mut self.draft, side);
                row(ui, "Diagonals", |ui| {
                    choice(ui, &format!("{id}_diag"), &mut pad.diagonals, &[
                        (Diagonals::EightWay, "8-way (with diagonals)"),
                        (Diagonals::FourWay, "4-way"),
                    ])
                });
            }
            PadMode::None => {
                self.binding_row(ui, "Click", Slot::PadClick(side));
            }
            _ => {
                self.binding_row(ui, "Click", Slot::PadClick(side));
                self.binding_row(ui, "Touch / Tap", Slot::PadTouch(side));
            }
        }
    }

    fn trigger(&mut self, ui: &mut egui::Ui, side: usize) {
        let id = if side == 0 { "lt" } else { "rt" };
        let live = self.shared.live.trigger[side].load(Ordering::Relaxed);
        let game_mode = self.shared.status().game_mode;
        let dual = {
            let t = if side == 0 { &mut self.draft.left_trigger } else { &mut self.draft.right_trigger };
            row(ui, "Trigger Mode", |ui| {
                choice(ui, &format!("{id}_mode"), &mut t.mode, &[
                    (TriggerMode::Standard, "Standard"),
                    (TriggerMode::DualStage, "Dual-stage (extra action at full press)"),
                ])
            });
            t.is_dual()
        };
        if !dual {
            meter(ui, live, None, game_mode);
            return;
        }
        {
            let t = if side == 0 { &mut self.draft.left_trigger } else { &mut self.draft.right_trigger };
            ui.label(RichText::new("Full-Press Point").color(LABEL));
            meter(ui, live, Some((t.press, t.release)), game_mode);
            row(ui, "Press at", |ui| {
                ui.add(egui::Slider::new(&mut t.press, TRIGGER_PRESS_MIN..=TRIGGER_PRESS_MAX).suffix("%"));
            });
            t.release = clamp_trigger_release(t.release, t.press);
            let ceiling = t.press.saturating_sub(TRIGGER_HYSTERESIS_MIN).max(TRIGGER_RELEASE_MIN);
            row(ui, "Release below", |ui| {
                ui.add(egui::Slider::new(&mut t.release, TRIGGER_RELEASE_MIN..=ceiling).suffix("%"));
            });
            row(ui, "Click Feel", |ui| {
                choice(ui, &format!("{id}_haptic"), &mut t.haptic, &[
                    (TriggerHaptic::Off, "Off"),
                    (TriggerHaptic::Press, "Press"),
                    (TriggerHaptic::PressAndRelease, "Press and release"),
                ])
            });
        }
        self.binding_row(ui, "Full press does", Slot::TriggerFull(side));
    }

    fn stick(&mut self, ui: &mut egui::Ui, side: usize) {
        let id = if side == 0 { "ls" } else { "rs" };
        let rx = self.shared.live.stick[side * 2].load(Ordering::Relaxed) as i16;
        let ry = self.shared.live.stick[side * 2 + 1].load(Ordering::Relaxed) as i16;
        let s = if side == 0 { &mut self.draft.left_stick } else { &mut self.draft.right_stick };
        ui.horizontal(|ui| {
            stick_preview(ui, rx, ry, s);
            ui.vertical(|ui| {
                row(ui, "Deadzone", |ui| {
                    choice(ui, &format!("{id}_mode"), &mut s.mode, &[
                        (StickMode::Default, "Default"),
                        (StickMode::Circle, "Circle"),
                        (StickMode::Square, "Square"),
                    ])
                });
                if s.mode == StickMode::Default {
                    hint(ui, &format!("A small {STICK_DEADZONE_PERCENT}% round deadzone."));
                } else {
                    let mut inner = s.inner;
                    let mut outer = s.outer;
                    row(ui, "Inner", |ui| {
                        ui.add(egui::Slider::new(&mut inner, 0..=(100 - STICK_MIN_GAP)).suffix("%"));
                    });
                    row(ui, "Outer", |ui| {
                        ui.add(egui::Slider::new(&mut outer, STICK_MIN_GAP..=100).suffix("%"));
                    });
                    if outer != s.outer && outer < inner + STICK_MIN_GAP {
                        inner = outer.saturating_sub(STICK_MIN_GAP);
                    }
                    clamp_stick_range(&mut inner, &mut outer);
                    s.inner = inner;
                    s.outer = outer;
                }
            });
        });
    }

    fn actions(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Reset this profile to default").clicked() {
                let keep = self.draft.use_default_mappings;
                self.draft = Profile::default();
                self.draft.use_default_mappings = keep && self.target != Target::Default;
                self.cancel_listen();
            }
            if self.target != Target::Default {
                let del = egui::Button::new(RichText::new("Delete Profile").color(Color32::WHITE)).fill(DANGER);
                if ui.add(del).clicked() {
                    self.delete_profile();
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let dirty = self.dirty();
                if green_button(ui, "Save", dirty).clicked() {
                    self.save();
                }
                if ui.add_enabled(dirty, egui::Button::new("Cancel").min_size(Vec2::new(80.0, 30.0))).clicked() {
                    self.revert();
                }
                if let Some((m, t)) = &self.message {
                    if t.elapsed() < Duration::from_secs(3) {
                        ui.label(RichText::new(m).color(ACCENT));
                    }
                } else if dirty {
                    hint(ui, "Unsaved changes");
                }
            });
        });
    }

    fn unsaved_modal(&mut self, ctx: &egui::Context) {
        if self.pending.is_none() {
            return;
        }
        let mut choice: Option<bool> = None;
        let mut keep_editing = false;
        egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(RichText::new("Unsaved changes").size(16.0).strong().color(Color32::WHITE));
            ui.label(format!("Save the changes to \"{}\"?", self.target_name()));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Keep editing").clicked() {
                    keep_editing = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if green_button(ui, "Save", true).clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some(false);
                    }
                });
            });
        });
        if keep_editing {
            self.pending = None;
            return;
        }
        if let Some(save) = choice {
            if save {
                self.save();
            } else {
                self.draft = self.saved.clone();
                self.new_game = None;
                if matches!(self.target, Target::Game(_))
                    && !self.shared.settings.read().unwrap().games.iter().any(|g| Target::Game(g.matcher.key()) == self.target)
                {
                    self.load_target(Target::Default);
                }
            }
            if let Some(p) = self.pending.take() {
                self.run_pending(p);
            }
        }
    }
}

fn badge(ui: &mut egui::Ui, b: &Binding, ps: bool) {
    let (text, colour) = match b {
        Binding::Action(Action::None) => ("OFF".to_string(), DIM),
        Binding::Action(Action::LeftMouse | Action::RightMouse) | Binding::Mouse(_) => ("MOUSE".into(), ACCENT),
        Binding::Action(Action::TouchKeyboard) => ("DESKTOP".into(), ACCENT),
        Binding::Key { .. } => ("KEY".into(), Color32::from_rgb(0xe0, 0xb0, 0x4a)),
        Binding::Action(_) => (if ps { "PAD".into() } else { "PAD".into() }, METER_HOT),
    };
    egui::Frame::new()
        .stroke(Stroke::new(1.0, colour))
        .corner_radius(3)
        .inner_margin(egui::Margin::symmetric(5, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.0).strong().color(colour));
        });
}

fn meter(ui: &mut egui::Ui, raw: i32, marks: Option<(u32, u32)>, live: bool) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 16.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, SELECT);
    let frac = (raw as f32 / TRIGGER_RAW_MAX as f32).clamp(0.0, 1.0);
    let hot = marks.is_some_and(|(press, _)| raw >= trigger_raw_at(press));
    let fill = Rect::from_min_size(rect.min, Vec2::new(rect.width() * frac, rect.height()));
    p.rect_filled(fill, 4.0, if hot { METER_HOT } else { METER });
    if let Some((press, release)) = marks {
        for (pct, c) in [(press, Color32::WHITE), (release, DIM)] {
            let x = rect.left() + rect.width() * pct as f32 / 100.0;
            p.line_segment([Pos2::new(x, rect.top() - 2.0), Pos2::new(x, rect.bottom() + 2.0)], Stroke::new(2.0, c));
        }
    }
    let text = if live { format!("{:.0}%", frac * 100.0) } else { "Pull the trigger while Steamless Mode is on to see it here".into() };
    p.text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(11.0), TEXT);
}

fn stick_preview(ui: &mut egui::Ui, x: i16, y: i16, s: &StickSettings) {
    let size = 120.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    let r = size / 2.0 - 4.0;
    p.rect_filled(rect, 8.0, SELECT);
    if s.mode == StickMode::Square {
        let d = |pct: u32| Rect::from_center_size(c, Vec2::splat(2.0 * r * pct as f32 / 100.0));
        p.rect_stroke(d(s.outer), 0.0, Stroke::new(1.0, DIM), egui::StrokeKind::Middle);
        p.rect_filled(d(s.inner), 0.0, Color32::from_rgba_unmultiplied(192, 57, 43, 60));
    } else {
        let (inner, outer) = if s.mode == StickMode::Default { (STICK_DEADZONE_PERCENT as u32, 100) } else { (s.inner, s.outer) };
        p.circle_stroke(c, r * outer as f32 / 100.0, Stroke::new(1.0, DIM));
        p.circle_filled(c, r * inner as f32 / 100.0, Color32::from_rgba_unmultiplied(192, 57, 43, 60));
    }
    p.circle_stroke(c, r, Stroke::new(1.0, row_border()));
    let to_pos = |vx: i16, vy: i16| Pos2::new(c.x + r * vx as f32 / 32767.0, c.y - r * vy as f32 / 32767.0);
    p.circle_filled(to_pos(x, y), 4.0, STICK_RAW);
    let (ox, oy) = apply_stick(x, y, s);
    p.circle_filled(to_pos(ox, oy), 5.0, STICK_OUT);
}
