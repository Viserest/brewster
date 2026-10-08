//! Interactive terminal viewer (crossterm): scrolling, reload, live resize.

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{
        Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
    terminal::{
        self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use engine::{Rgb, Row, render_source};
use std::fs;
use std::io::{self, Write};

fn io_err(e: io::Error) -> String {
    e.to_string()
}

/// Restores the terminal on every exit path, including panics and early returns.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

fn read_and_render(path: &str, width: usize) -> Result<(String, Vec<Row>), String> {
    let src = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path, e))?;
    let rows = render_source(&src, width)?;
    Ok((src, rows))
}

fn draw(
    out: &mut io::Stdout,
    rows: &[Row],
    top: usize,
    view_h: usize,
    width: usize,
    path: &str,
    notice: &Option<String>,
) -> io::Result<()> {
    for y in 0..view_h {
        queue!(out, MoveTo(0, y as u16))?;
        if let Some(row) = rows.get(top + y) {
            for s in row {
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
                queue!(out, Print(&s.text))?;
            }
            queue!(out, SetAttribute(Attribute::Reset), ResetColor)?;
        }
        queue!(out, Clear(ClearType::UntilNewLine))?;
    }

    let total = rows.len();
    let first = if total == 0 { 0 } else { top + 1 };
    let last = (top + view_h).min(total);
    let msg = match notice {
        Some(n) => n.replace('\n', " "),
        None => format!(
            "{}  lines {}-{}/{}  q quit  r reload  j/k scroll  space/b page  g/G ends",
            path, first, last, total
        ),
    };
    let room = width.saturating_sub(1);
    let mut line: String = msg.chars().take(room).collect();
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
    let mut width = (cols as usize).max(1);
    let mut height = lines as usize;
    // Load before touching the terminal so a bad file reports a normal error.
    let (mut src, mut rows) = read_and_render(path, width)?;
    let mut top = 0usize;
    let mut notice: Option<String> = None;

    enable_raw_mode().map_err(io_err)?;
    let _guard = TerminalGuard;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, Hide).map_err(io_err)?;

    loop {
        let view_h = height.saturating_sub(1).max(1);
        let max_top = rows.len().saturating_sub(view_h);
        top = top.min(max_top);
        draw(&mut out, &rows, top, view_h, width, path, &notice).map_err(io_err)?;

        match event::read().map_err(io_err)? {
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind,
                ..
            }) if kind != KeyEventKind::Release => {
                notice = None;
                match code {
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Enter => {
                        top = top.saturating_add(1)
                    }
                    KeyCode::Up | KeyCode::Char('k') => top = top.saturating_sub(1),
                    KeyCode::PageDown | KeyCode::Char(' ') => top = top.saturating_add(view_h),
                    KeyCode::PageUp | KeyCode::Char('b') => top = top.saturating_sub(view_h),
                    KeyCode::Home | KeyCode::Char('g') => top = 0,
                    KeyCode::End | KeyCode::Char('G') => top = max_top,
                    KeyCode::Char('r') => match read_and_render(path, width) {
                        Ok((s, r)) => {
                            src = s;
                            rows = r;
                            notice = Some("reloaded".to_string());
                        }
                        Err(e) => notice = Some(e),
                    },
                    _ => {}
                }
            }
            Event::Resize(c, l) => {
                width = (c as usize).max(1);
                height = l as usize;
                match render_source(&src, width) {
                    Ok(r) => rows = r,
                    Err(e) => notice = Some(e),
                }
            }
            _ => {}
        }
    }
    Ok(())
}
