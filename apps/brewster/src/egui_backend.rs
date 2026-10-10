//! egui / eframe backend: a window that paints the viewer's cell grid.
//!
//! Input: printable characters arrive as `Event::Text`, everything else as `Event::Key`;
//! the platform's copy and paste commands arrive as `Event::Copy` / `Event::Paste` (egui
//! reads the system clipboard for paste). Copied text goes out with `Context::copy_text`.

use crate::grid::{self, Metrics};
use eframe::egui;
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, Vec2};
use engine::{Rgb, Span};
use viewer::{Flow, Key, KeyPress, Mods, Viewer, paint};

const BG: Color32 = Color32::from_rgb(24, 24, 24);
const FG: Color32 = Color32::from_rgb(220, 220, 220);
const FONT_SIZE: f32 = 15.0;

pub fn run(path: &str) -> Result<(), String> {
    // Load before opening a window so a bad file reports a normal error.
    let mut viewer = Viewer::new(path, 80, 25)?;
    if let Some(e) = viewer.notice.take() {
        return Err(e);
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("brewster - {}", path))
            .with_inner_size([960.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "brewster",
        options,
        Box::new(move |_cc| Ok(Box::new(GuiApp::new(viewer)))),
    )
    .map_err(|e| e.to_string())
}

struct GuiApp {
    viewer: Viewer,
    font: FontId,
}

impl GuiApp {
    fn new(viewer: Viewer) -> GuiApp {
        GuiApp {
            viewer,
            font: FontId::monospace(FONT_SIZE),
        }
    }

    fn metrics(&self, ctx: &egui::Context) -> Metrics {
        ctx.fonts(|f| Metrics {
            cell_w: f.glyph_width(&self.font, 'M'),
            cell_h: f.row_height(&self.font),
        })
    }

    fn handle_input(&mut self, ctx: &egui::Context, m: Metrics) {
        let (events, wheel) = ctx.input(|i| (i.events.clone(), i.raw_scroll_delta.y));
        let mut quit = false;
        for ev in events {
            match ev {
                egui::Event::Text(s) => {
                    for c in s.chars().filter(|c| !c.is_control()) {
                        quit |= self.viewer.on_key(KeyPress::plain(Key::Char(c))) == Flow::Quit;
                    }
                }
                egui::Event::Paste(s) => self.viewer.on_paste(s),
                egui::Event::Copy | egui::Event::Cut => self.viewer.copy_selection(),
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if let Some(press) = map_key(key, modifiers) {
                        quit |= self.viewer.on_key(press) == Flow::Quit;
                    }
                }
                _ => {}
            }
        }
        if wheel != 0.0 {
            self.viewer.scroll_lines(grid::scroll_lines(wheel, m.cell_h));
        }
        if let Some(text) = self.viewer.take_copy() {
            ctx.copy_text(text);
        }
        if quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn draw_page(&mut self, ui: &mut egui::Ui, m: Metrics) {
        let avail = ui.available_size();
        let (cols, rows) = grid::grid_size(avail.x, avail.y, m);
        // The viewer counts one extra row for a status line; here the status is a separate bar.
        if (cols, rows + 1) != (self.viewer.width, self.viewer.height) {
            self.viewer.on_resize(cols, rows + 1);
        }

        let (rect, _) = ui.allocate_exact_size(avail, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let highlight = self.viewer.highlight();
        let editing = self.viewer.editing().is_some();

        for y in 0..self.viewer.view_h() {
            let idx = self.viewer.top + y;
            let Some(row) = self.viewer.page.rows.get(idx) else {
                break;
            };
            let top = grid::y_of(rect.min.y, y, m);
            let mut col = 0usize;
            for (span, reversed) in paint::row_pieces(row, idx, highlight.as_ref(), editing) {
                let pos = Pos2::new(grid::x_of(rect.min.x, col, m), top);
                col += span.text.chars().count();
                draw_piece(&painter, pos, m, &self.font, &span, reversed);
            }
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let m = self.metrics(ctx);
        self.handle_input(ctx, m);

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(egui::RichText::new(self.viewer.status()).monospace());
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG))
            .show(ctx, |ui| self.draw_page(ui, m));
    }
}

fn color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

fn draw_piece(
    painter: &egui::Painter,
    pos: Pos2,
    m: Metrics,
    font: &FontId,
    span: &Span,
    reversed: bool,
) {
    if span.text.is_empty() {
        return;
    }
    let width = span.text.chars().count() as f32 * m.cell_w;
    let mut fg = span.fg.map(color);
    let mut bg = span.bg.map(color);
    if reversed {
        let (f, b) = (fg.unwrap_or(FG), bg.unwrap_or(BG));
        fg = Some(b);
        bg = Some(f);
    }
    if let Some(b) = bg {
        painter.rect_filled(Rect::from_min_size(pos, Vec2::new(width, m.cell_h)), 0.0, b);
    }
    let fg = fg.unwrap_or(FG);
    painter.text(pos, Align2::LEFT_TOP, &span.text, font.clone(), fg);
    if span.bold {
        // The default monospace font has no bold face: strike it twice, a fraction apart.
        painter.text(
            Pos2::new(pos.x + 0.7, pos.y),
            Align2::LEFT_TOP,
            &span.text,
            font.clone(),
            fg,
        );
    }
    if span.underline {
        let y = pos.y + m.cell_h - 1.5;
        painter.line_segment(
            [Pos2::new(pos.x, y), Pos2::new(pos.x + width, y)],
            Stroke::new(1.0_f32, fg),
        );
    }
}

/// Keys that arrive as key events. Printable characters come as `Event::Text` instead, and
/// Ctrl+C / Ctrl+V as `Event::Copy` / `Event::Paste`, so they are not mapped here.
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
        // Emacs-style line start / end while editing an input.
        K::A if m.ctrl => Key::Char('a'),
        K::E if m.ctrl => Key::Char('e'),
        _ => return None,
    };
    Some(KeyPress::new(
        k,
        Mods {
            ctrl: m.ctrl,
            alt: m.alt,
        },
    ))
}
