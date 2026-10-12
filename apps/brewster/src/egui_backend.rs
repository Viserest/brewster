//! egui / eframe backend: a window that paints the browser's cell screen.
//!
//! The browser composes everything (tab bar, page, settings box, status line) into a grid
//! of cells; this file only draws that grid and feeds input back.
//!
//! Drawing: every cell edge sits on a whole physical pixel (see `grid`), backgrounds are
//! filled first as merged runs, then each glyph is drawn in its own cell, so text never
//! drifts out of its cell (no chopped last character) and neighbouring fills never leave
//! seams (no grid lines). A header glyph with a font size above 1 starts in one cell and
//! covers several, so glyphs are drawn after all backgrounds.
//!
//! Input: printable characters arrive as `Event::Text`; ctrl/alt chords and named keys as
//! `Event::Key`; the platform's copy and paste commands as `Event::Copy` / `Event::Paste`
//! (egui reads the system clipboard for paste); the mouse as `Event::PointerButton`.

use crate::grid::{self, Metrics, Wheel};

use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2, Rect};

use browser::{
    Browser, Button, Capabilities, Cell, Effect, Key, KeyPress, Mods, MouseEvent, MouseKind,
    Screen,
    launch::{self, Options},
};
use engine::Rgb;
use profile::Paths;

const FONT_SIZE: f32 = 15.0;
/// The grid the first frame is laid out in, before the window's real size is known.
const FIRST_GRID: (usize, usize) = (100, 35);

pub fn run(opts: Options) -> Result<(), String> {
    let caps = Capabilities {
        windows: true,
        scaling: true,
    };
    let (browser, warnings) =
        launch::open(&opts, &Paths::from_env(), FIRST_GRID.0, FIRST_GRID.1, caps)?;
    for w in warnings {
        eprintln!("warning: {}", w);
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("brewster")
            .with_inner_size([960.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "brewster",
        options,
        Box::new(move |_cc| Ok(Box::new(GuiApp::new(browser, opts)))),
    )
    .map_err(|e| e.to_string())
}

struct GuiApp {
    browser: Browser,
    opts: Options,
    font: FontId,
    /// The grid the browser is laid out in: the window's size in cells.
    grid: (usize, usize),
    wheel: Wheel,
    title: String,
}

impl GuiApp {
    fn new(browser: Browser, opts: Options) -> GuiApp {
        GuiApp {
            browser,
            opts,
            font: FontId::monospace(FONT_SIZE),
            grid: FIRST_GRID,
            wheel: Wheel::default(),
            title: String::new(),
        }
    }

    /// One cell, measured from a long line of glyphs so the advance is exact rather than
    /// that of a single glyph rounded by the font engine.
    fn metrics(&self, ctx: &egui::Context) -> Metrics {
        const N: usize = 64;
        let (adv, line_h) = ctx.fonts(|f| {
            let g = f.layout_no_wrap("M".repeat(N), self.font.clone(), Color32::WHITE);
            (g.size().x / N as f32, f.row_height(&self.font))
        });
        Metrics::measured(adv, line_h, ctx.pixels_per_point())
    }

    fn frame(&mut self, ctx: &egui::Context, ui: &egui::Ui, rect: Rect) {
        let m = self.metrics(ctx);
        let origin = Pos2::new(grid::snap(rect.min.x, m.ppp), grid::snap(rect.min.y, m.ppp));
        // The page is as big as the window: every whole cell that fits.
        let (cols, rows) = grid::grid_size(rect.max.x - origin.x, rect.max.y - origin.y, m);
        if (cols, rows) != self.grid {
            self.grid = (cols, rows);
            self.browser.on_resize(cols, rows);
        }

        self.handle_input(ctx, rect, origin, m);
        self.carry_out(ctx);
        self.update_title(ctx);

        let screen = self.browser.screen();
        paint(&ui.painter_at(rect), &screen, origin, m, &self.font);
    }

    fn handle_input(&mut self, ctx: &egui::Context, rect: Rect, origin: Pos2, m: Metrics) {
        let (events, hover, wheel, held) = ctx.input(|i| {
            (
                i.events.clone(),
                i.pointer.hover_pos(),
                i.raw_scroll_delta.y,
                i.modifiers,
            )
        });
        let cell_of = |p: Pos2| -> Option<(usize, usize)> {
            if rect.contains(p) {
                grid::cell_at(origin.x, origin.y, p.x, p.y, m)
            } else {
                None
            }
        };

        for ev in events {
            match ev {
                egui::Event::Text(s) => {
                    // A ctrl or alt chord arrives as a key event as well: not typed text.
                    // (AltGr is reported as both, and does type.)
                    if is_chord(held) {
                        continue;
                    }
                    for c in s.chars().filter(|c| !c.is_control()) {
                        self.browser.on_key(KeyPress::plain(Key::Char(c)));
                    }
                }
                egui::Event::Paste(s) => self.browser.on_paste(s),
                egui::Event::Copy | egui::Event::Cut => self.browser.copy_selection(),
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if let Some(press) = map_key(key, modifiers) {
                        self.browser.on_key(press);
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers,
                } => {
                    let button = match button {
                        egui::PointerButton::Primary => Button::Left,
                        egui::PointerButton::Middle => Button::Middle,
                        egui::PointerButton::Secondary => Button::Right,
                        _ => continue,
                    };
                    if let Some((col, row)) = cell_of(pos) {
                        self.browser.on_mouse(MouseEvent {
                            col,
                            row,
                            kind: MouseKind::Down(button),
                            mods: mods(modifiers),
                        });
                    }
                }
                _ => {}
            }
        }

        let steps = self.wheel.push(wheel, m.cell_h);
        if steps != 0 {
            if let Some((col, row)) = hover.and_then(cell_of) {
                let dir = steps.signum();
                for _ in 0..steps.abs() {
                    self.browser.on_mouse(MouseEvent {
                        col,
                        row,
                        kind: MouseKind::Wheel(dir),
                        mods: Mods::default(),
                    });
                }
            }
        }
    }

    fn carry_out(&mut self, ctx: &egui::Context) {
        for e in self.browser.take_effects() {
            match e {
                Effect::Copy(text) => ctx.copy_text(text),
                Effect::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Effect::NewWindow(url) => {
                    if let Err(e) = open_window(&self.opts, &url) {
                        eprintln!("warning: {}", e);
                    }
                }
            }
        }
        if self.browser.should_quit() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = match self.browser.tab_title(self.browser.active_tab()) {
            Some(t) if !t.is_empty() => format!("{} - brewster", t),
            _ => "brewster".to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(color(grid::THEME_BG)))
            .show(ctx, |ui| {
                let rect = ui.available_rect_before_wrap();
                self.frame(ctx, ui, rect);
            });
    }
}

/// Another window is another process on the same profile. It does not remember its tabs at
/// quit (`--follower`), so it cannot overwrite this window's session.
fn open_window(opts: &Options, url: &str) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot open a window: {}", e))?;
    std::process::Command::new(exe)
        .args(opts.profile_args())
        .arg("--follower")
        .arg("--")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("cannot open a window: {}", e))
}

fn color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

fn mods(m: egui::Modifiers) -> Mods {
    Mods {
        ctrl: m.ctrl,
        alt: m.alt,
        shift: m.shift,
    }
}

/// Exactly one of Ctrl and Alt: a shortcut, as opposed to typing (AltGr is both).
fn is_chord(m: egui::Modifiers) -> bool {
    m.ctrl != m.alt || m.command
}

fn colors(c: &Cell) -> (Rgb, Option<Rgb>) {
    grid::resolve(c.fg, c.bg, c.reverse)
}

fn paint(painter: &egui::Painter, screen: &Screen, origin: Pos2, m: Metrics, font: &FontId) {
    // 1. Backgrounds, one rectangle per run of equal color. Cell edges are on whole pixels,
    //    so rectangles of neighbouring runs and rows meet exactly.
    for y in 0..screen.rows {
        let top = grid::y_of(origin.y, y, m);
        let bottom = grid::y_of(origin.y, y + 1, m);
        let bg_at = |x: usize| screen.get(x, y).and_then(|c| colors(c).1);
        let mut x = 0;
        while x < screen.cols {
            let bg = bg_at(x);
            let mut end = x + 1;
            while end < screen.cols && bg_at(end) == bg {
                end += 1;
            }
            if let Some(b) = bg {
                let r = Rect::from_min_max(
                    Pos2::new(grid::x_of(origin.x, x, m), top),
                    Pos2::new(grid::x_of(origin.x, end, m), bottom),
                );
                painter.rect_filled(r, 0.0, color(b));
            }
            x = end;
        }
    }

    // 2. Glyphs and underlines, each in its own cell. A scaled glyph is drawn at a larger
    //    font size from its first cell and overlaps the cells it covers.
    for y in 0..screen.rows {
        let top = grid::y_of(origin.y, y, m);
        for x in 0..screen.cols {
            let Some(c) = screen.get(x, y) else { continue };
            if c.cover {
                continue;
            }
            let scale = c.scale.max(1) as f32;
            let (fg, _) = colors(c);
            let left = grid::x_of(origin.x, x, m);
            let pos = Pos2::new(left, top);
            if c.ch != ' ' {
                let f = if c.scale > 1 {
                    FontId::monospace(FONT_SIZE * scale)
                } else {
                    font.clone()
                };
                painter.text(pos, Align2::LEFT_TOP, c.ch, f.clone(), color(fg));
                if c.bold {
                    // The default monospace font has no bold face: strike it a pixel apart.
                    painter.text(
                        Pos2::new(left + m.px() * scale, top),
                        Align2::LEFT_TOP,
                        c.ch,
                        f,
                        color(fg),
                    );
                }
            }
            if c.underline {
                let thick = m.px() * scale;
                let y_line = grid::snap(top + scale * m.cell_h - 2.0 * thick, m.ppp);
                let r = Rect::from_min_max(
                    Pos2::new(left, y_line),
                    Pos2::new(left + scale * m.cell_w, y_line + thick),
                );
                painter.rect_filled(r, 0.0, color(fg));
            }
        }
    }
}

/// Keys that arrive as key events: named keys, function keys, and letters or digits held
/// with Ctrl or Alt. Typed characters come as `Event::Text` instead, and Ctrl+C / V / X as
/// `Event::Copy` / `Paste` / `Cut` (so they are not mapped: they must not quit or paste twice).
fn map_key(key: egui::Key, m: egui::Modifiers) -> Option<KeyPress> {
    use egui::Key as K;
    let k = match key {
        K::Escape => Key::Esc,
        K::Enter => Key::Enter,
        K::Tab if m.shift => Key::BackTab,
        K::Tab => Key::Tab,
        K::ArrowUp => Key::Up,
        K::ArrowDown => Key::Down,
        K::ArrowLeft => Key::Left,
        K::ArrowRight => Key::Right,
        K::Home => Key::Home,
        K::End => Key::End,
        K::PageUp => Key::PageUp,
        K::PageDown => Key::PageDown,
        K::Backspace => Key::Backspace,
        K::Delete => Key::Delete,
        K::F1 => Key::F(1),
        K::F2 => Key::F(2),
        K::F3 => Key::F(3),
        K::F4 => Key::F(4),
        K::F5 => Key::F(5),
        K::F6 => Key::F(6),
        K::F7 => Key::F(7),
        K::F8 => Key::F(8),
        K::F9 => Key::F(9),
        K::F10 => Key::F(10),
        K::F11 => Key::F(11),
        K::F12 => Key::F(12),
        other => {
            if !is_chord(m) {
                return None;
            }
            let c = chord_char(other)?;
            if m.ctrl && matches!(c, 'c' | 'v' | 'x') {
                return None;
            }
            Key::Char(c)
        }
    };
    Some(KeyPress::new(k, mods(m)))
}

fn chord_char(key: egui::Key) -> Option<char> {
    use egui::Key as K;
    Some(match key {
        K::A => 'a',
        K::B => 'b',
        K::C => 'c',
        K::D => 'd',
        K::E => 'e',
        K::F => 'f',
        K::G => 'g',
        K::H => 'h',
        K::I => 'i',
        K::J => 'j',
        K::K => 'k',
        K::L => 'l',
        K::M => 'm',
        K::N => 'n',
        K::O => 'o',
        K::P => 'p',
        K::Q => 'q',
        K::R => 'r',
        K::S => 's',
        K::T => 't',
        K::U => 'u',
        K::V => 'v',
        K::W => 'w',
        K::X => 'x',
        K::Y => 'y',
        K::Z => 'z',
        K::Num0 => '0',
        K::Num1 => '1',
        K::Num2 => '2',
        K::Num3 => '3',
        K::Num4 => '4',
        K::Num5 => '5',
        K::Num6 => '6',
        K::Num7 => '7',
        K::Num8 => '8',
        K::Num9 => '9',
        _ => return None,
    })
}
