//! Interactive terminal viewer (crossterm): terminal setup, painting the browser's screen
//! and the event loop. Everything else lives in the `browser` crate.

use crate::clipboard;
use crate::keys;

use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event,
    },
    execute, queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
    terminal::{
        self, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};

use browser::{
    Browser, Capabilities, Effect, Screen,
    launch::{self, Options},
};
use engine::Rgb;
use profile::Paths;

fn io_err(e: io::Error) -> String {
    e.to_string()
}

/// Restores the terminal on every exit path, including panics and early returns.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            Show,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
    }
}

/// Neighbouring cells with the same look, painted with one escape sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub underline: bool,
    pub reverse: bool,
}

/// Splits one screen row into runs. Cells covered by a scaled glyph are blanks here (a
/// terminal cannot draw other font sizes).
pub fn runs(screen: &Screen, y: usize) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for x in 0..screen.cols {
        let Some(c) = screen.get(x, y) else { continue };
        let ch = if c.cover { ' ' } else { c.ch };
        match out.last_mut() {
            Some(r)
                if r.fg == c.fg
                    && r.bg == c.bg
                    && r.bold == c.bold
                    && r.underline == c.underline
                    && r.reverse == c.reverse =>
            {
                r.text.push(ch)
            }
            _ => out.push(Run {
                text: ch.to_string(),
                fg: c.fg,
                bg: c.bg,
                bold: c.bold,
                underline: c.underline,
                reverse: c.reverse,
            }),
        }
    }
    out
}

fn draw(out: &mut io::Stdout, screen: &Screen) -> io::Result<()> {
    for y in 0..screen.rows {
        queue!(out, MoveTo(0, y as u16))?;
        for r in runs(screen, y) {
            queue!(out, SetAttribute(Attribute::Reset), ResetColor)?;
            if r.bold {
                queue!(out, SetAttribute(Attribute::Bold))?;
            }
            if r.underline {
                queue!(out, SetAttribute(Attribute::Underlined))?;
            }
            if let Some(Rgb(red, g, b)) = r.fg {
                queue!(out, SetForegroundColor(Color::Rgb { r: red, g, b }))?;
            }
            if let Some(Rgb(red, g, b)) = r.bg {
                queue!(out, SetBackgroundColor(Color::Rgb { r: red, g, b }))?;
            }
            if r.reverse {
                queue!(out, SetAttribute(Attribute::Reverse))?;
            }
            queue!(out, Print(&r.text))?;
        }
        queue!(out, SetAttribute(Attribute::Reset), ResetColor)?;
    }
    out.flush()
}

/// Runs what the browser asked for. Returns true to quit.
fn carry_out(out: &mut io::Stdout, app: &mut Browser) -> Result<bool, String> {
    let mut quit = false;
    for e in app.take_effects() {
        match e {
            Effect::Copy(text) => {
                write!(out, "{}", clipboard::osc52(&text)).map_err(io_err)?;
                out.flush().map_err(io_err)?;
            }
            Effect::Quit => quit = true,
            // A terminal has no windows: the browser opens a tab instead and never asks.
            Effect::NewWindow(_) => {}
        }
    }
    Ok(quit || app.should_quit())
}

pub fn run(opts: &Options) -> Result<(), String> {
    let (cols, lines) = terminal::size().map_err(io_err)?;
    // The first page is as big as the terminal.
    let (mut app, warnings) = launch::open(
        opts,
        &Paths::from_env(),
        cols as usize,
        lines as usize,
        Capabilities::default(),
    )?;

    enable_raw_mode().map_err(io_err)?;
    {
        let _guard = TerminalGuard;
        let mut out = io::stdout();
        execute!(
            out,
            EnterAlternateScreen,
            Hide,
            EnableBracketedPaste,
            EnableMouseCapture
        )
        .map_err(io_err)?;

        loop {
            draw(&mut out, &app.screen()).map_err(io_err)?;

            match event::read().map_err(io_err)? {
                Event::Key(k) => {
                    if let Some(press) = keys::map(k) {
                        app.on_key(press);
                    }
                }
                Event::Mouse(m) => {
                    if let Some(ev) = keys::map_mouse(m) {
                        app.on_mouse(ev);
                    }
                }
                Event::Paste(s) => app.on_paste(s),
                Event::Resize(c, l) => app.on_resize(c as usize, l as usize),
                _ => {}
            }
            if carry_out(&mut out, &mut app)? {
                break;
            }
        }
    }
    for w in warnings {
        eprintln!("warning: {}", w);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use browser::{Cell, Look};

    #[test]
    fn equal_neighbours_share_a_run() {
        let mut s = Screen::new(6, 1);
        s.text(0, 0, "ab", Look::PLAIN, 6);
        s.text(2, 0, "cd", Look::REVERSE, 6);
        let r = runs(&s, 0);
        assert_eq!(r.len(), 3, "{:?}", r);
        assert_eq!(r[0].text, "ab");
        assert!(!r[0].reverse);
        assert_eq!(r[1].text, "cd");
        assert!(r[1].reverse);
        assert_eq!(r[2].text, "  ");
        // Every column is painted exactly once, including the last.
        let total: usize = r.iter().map(|x| x.text.chars().count()).sum();
        assert_eq!(total, 6);
    }

    #[test]
    fn colors_split_runs_and_covered_cells_are_blank() {
        let mut s = Screen::new(3, 1);
        s.set(
            0,
            0,
            Cell {
                ch: 'x',
                fg: Some(Rgb(1, 2, 3)),
                ..Cell::default()
            },
        );
        s.set(
            1,
            0,
            Cell {
                ch: 'y',
                cover: true,
                ..Cell::default()
            },
        );
        let r = runs(&s, 0);
        assert_eq!(r[0].text, "x");
        assert_eq!(r[0].fg, Some(Rgb(1, 2, 3)));
        assert_eq!(r[1].text, "  ");
    }
}
