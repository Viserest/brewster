//! Interactive terminal viewer (crossterm): terminal setup, drawing and the event loop.
//! State and key handling live in the `viewer` crate.

use crate::clipboard;
use crate::keys;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event},
    execute, queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
    terminal::{
        self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use engine::Rgb;
use std::io::{self, Write};
use viewer::{Flow, Viewer, paint};

fn io_err(e: io::Error) -> String {
    e.to_string()
}

/// Restores the terminal on every exit path, including panics and early returns.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableBracketedPaste, Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

fn draw(out: &mut io::Stdout, app: &Viewer) -> io::Result<()> {
    let view_h = app.view_h();
    let highlight = app.highlight();
    let editing = app.editing().is_some();

    for y in 0..view_h {
        queue!(out, MoveTo(0, y as u16))?;
        let idx = app.top + y;
        if let Some(row) = app.page.rows.get(idx) {
            for (s, reversed) in paint::row_pieces(row, idx, highlight.as_ref(), editing) {
                if s.text.is_empty() {
                    continue;
                }
                queue!(out, SetAttribute(Attribute::Reset), ResetColor)?;
                if s.bold {
                    queue!(out, SetAttribute(Attribute::Bold))?;
                }
                if s.underline {
                    queue!(out, SetAttribute(Attribute::Underlined))?;
                }
                if let Some(Rgb(r, g, b)) = s.fg {
                    queue!(out, SetForegroundColor(Color::Rgb { r, g, b }))?;
                }
                if let Some(Rgb(r, g, b)) = s.bg {
                    queue!(out, SetBackgroundColor(Color::Rgb { r, g, b }))?;
                }
                if reversed {
                    queue!(out, SetAttribute(Attribute::Reverse))?;
                }
                queue!(out, Print(&s.text))?;
            }
            queue!(out, SetAttribute(Attribute::Reset), ResetColor)?;
        }
        queue!(out, Clear(ClearType::UntilNewLine))?;
    }

    let room = app.width.saturating_sub(1);
    let mut line: String = app.status().chars().take(room).collect();
    let used = line.chars().count();
    line.push_str(&" ".repeat(room.saturating_sub(used)));
    queue!(
        out,
        MoveTo(0, view_h as u16),
        SetAttribute(Attribute::Reverse),
        Print(line),
        SetAttribute(Attribute::Reset)
    )?;
    out.flush()
}

pub fn run(path: &str) -> Result<(), String> {
    let (cols, lines) = terminal::size().map_err(io_err)?;
    // Load before touching the terminal so a bad file reports a normal error.
    let mut app = Viewer::new(path, cols as usize, lines as usize)?;
    if let Some(e) = app.notice.take() {
        return Err(e);
    }

    enable_raw_mode().map_err(io_err)?;
    let _guard = TerminalGuard;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, Hide, EnableBracketedPaste).map_err(io_err)?;

    loop {
        draw(&mut out, &app).map_err(io_err)?;

        let flow = match event::read().map_err(io_err)? {
            Event::Key(k) => match keys::map(k) {
                Some(press) => app.on_key(press),
                None => Flow::Continue,
            },
            Event::Paste(s) => {
                app.on_paste(s);
                Flow::Continue
            }
            Event::Resize(c, l) => {
                app.on_resize(c as usize, l as usize);
                Flow::Continue
            }
            _ => Flow::Continue,
        };
        if let Some(text) = app.take_copy() {
            write!(out, "{}", clipboard::osc52(&text)).map_err(io_err)?;
            out.flush().map_err(io_err)?;
        }
        if flow == Flow::Quit {
            break;
        }
    }
    Ok(())
}
