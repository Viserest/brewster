//! The boxes drawn over the page: the settings search box and the address prompt.

use crate::keymap::Action;
use crate::screen::{Cell, Look, Screen};
use crate::{Browser, Effect};

use settings::{Kind, Value, option, search};
use viewer::{Key, KeyPress};

/// A single-line text field with a caret.
#[derive(Debug, Clone, Default)]
pub(crate) struct Field {
    pub text: Vec<char>,
    pub caret: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldEvent {
    None,
    Changed,
    Submit,
    Cancel,
}

impl Field {
    pub fn with(text: &str) -> Field {
        let text: Vec<char> = text.chars().collect();
        Field {
            caret: text.len(),
            text,
        }
    }

    pub fn string(&self) -> String {
        self.text.iter().collect()
    }

    pub fn insert(&mut self, s: &str) {
        for c in s.chars().filter(|c| !c.is_control()) {
            self.text.insert(self.caret, c);
            self.caret += 1;
        }
    }

    pub fn key(&mut self, k: KeyPress) -> FieldEvent {
        if k.mods.ctrl {
            return match k.key {
                Key::Char('a') => {
                    self.caret = 0;
                    FieldEvent::None
                }
                Key::Char('e') => {
                    self.caret = self.text.len();
                    FieldEvent::None
                }
                Key::Char('u') => {
                    self.text.clear();
                    self.caret = 0;
                    FieldEvent::Changed
                }
                _ => FieldEvent::None,
            };
        }
        if k.mods.alt {
            return FieldEvent::None;
        }
        match k.key {
            Key::Enter => FieldEvent::Submit,
            Key::Esc => FieldEvent::Cancel,
            Key::Char(c) if !c.is_control() => {
                self.text.insert(self.caret, c);
                self.caret += 1;
                FieldEvent::Changed
            }
            Key::Backspace if self.caret > 0 => {
                self.caret -= 1;
                self.text.remove(self.caret);
                FieldEvent::Changed
            }
            Key::Delete if self.caret < self.text.len() => {
                self.text.remove(self.caret);
                FieldEvent::Changed
            }
            Key::Left => {
                self.caret = self.caret.saturating_sub(1);
                FieldEvent::None
            }
            Key::Right => {
                self.caret = (self.caret + 1).min(self.text.len());
                FieldEvent::None
            }
            Key::Home => {
                self.caret = 0;
                FieldEvent::None
            }
            Key::End => {
                self.caret = self.text.len();
                FieldEvent::None
            }
            _ => FieldEvent::None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SettingsBox {
    pub query: Field,
    pub selected: usize,
    pub scroll: usize,
    /// Editing one option's value: the option key and the text being typed.
    pub edit: Option<(&'static str, Field)>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub(crate) enum Overlay {
    Settings(SettingsBox),
    Open(Field),
}

/// Where the settings box sits on the screen.
pub(crate) struct BoxGeometry {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    /// First list row (screen y) and how many list rows there are.
    pub list_y: usize,
    pub list_h: usize,
}

pub(crate) fn box_geometry(cols: usize, rows: usize) -> BoxGeometry {
    let w = cols.saturating_sub(4).clamp(1, 78).min(cols);
    let h = rows.saturating_sub(2).clamp(1, 22).min(rows);
    let x = (cols - w) / 2;
    let y = (rows - h) / 2;
    // border(1) + search(1) + rule(1) + list + rule(1) + description(1) + help(1) + border(1)
    let list_h = h.saturating_sub(7).max(1);
    BoxGeometry {
        x,
        y,
        w,
        h,
        list_y: y + 3,
        list_h,
    }
}

fn clip(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else if max == 0 {
        String::new()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

fn next_choice(choices: &'static [&'static str], current: &str) -> &'static str {
    let i = choices
        .iter()
        .position(|c| *c == current)
        .map_or(0, |i| i + 1)
        % choices.len();
    choices[i]
}

impl Browser {
    pub(crate) fn open_settings(&mut self) {
        self.overlay = Some(Overlay::Settings(SettingsBox::default()));
    }

    pub(crate) fn open_prompt(&mut self) {
        let url = self.tabs[self.active].url.to_string();
        let text = if url.starts_with("about:") {
            String::new()
        } else {
            url
        };
        self.overlay = Some(Overlay::Open(Field::with(&text)));
    }

    pub(crate) fn overlay_paste(&mut self, text: &str) {
        match &mut self.overlay {
            Some(Overlay::Open(f)) => f.insert(text),
            Some(Overlay::Settings(b)) => {
                if let Some((_, f)) = &mut b.edit {
                    f.insert(text);
                } else {
                    b.query.insert(text);
                    b.selected = 0;
                    b.scroll = 0;
                }
            }
            None => {}
        }
    }

    pub(crate) fn overlay_key(&mut self, k: KeyPress) {
        let Some(ov) = self.overlay.take() else {
            return;
        };
        match ov {
            Overlay::Open(mut f) => match f.key(k) {
                FieldEvent::Submit => {
                    let text = f.string();
                    if !text.trim().is_empty() {
                        self.navigate_input(text.trim());
                    }
                }
                FieldEvent::Cancel => {}
                _ => self.overlay = Some(Overlay::Open(f)),
            },
            Overlay::Settings(mut b) => {
                if self.settings_key(&mut b, k) {
                    self.overlay = Some(Overlay::Settings(b));
                }
            }
        }
    }

    /// Returns whether the box stays open.
    fn settings_key(&mut self, b: &mut SettingsBox, k: KeyPress) -> bool {
        if let Some((key, field)) = &mut b.edit {
            match field.key(k) {
                FieldEvent::Submit => {
                    let text = field.string();
                    let key = *key;
                    b.edit = None;
                    self.commit_option(b, key, &text);
                }
                FieldEvent::Cancel => {
                    b.edit = None;
                    b.message = "cancelled".to_string();
                }
                _ => {}
            }
            return true;
        }
        match self.keymap.key(k) {
            Some(Action::SaveSettings) => {
                b.message = match self.save_profile() {
                    Ok(m) => m,
                    Err(e) => e,
                };
                return true;
            }
            Some(Action::Settings) => return false,
            _ => {}
        }
        let hits = search(&b.query.string());
        let last = hits.len().saturating_sub(1);
        let g = box_geometry(self.cols, self.rows);
        match k.key {
            Key::Esc if !k.mods.ctrl => return false,
            Key::Up => b.selected = b.selected.saturating_sub(1),
            Key::Down | Key::Tab => b.selected = (b.selected + 1).min(last),
            Key::BackTab => b.selected = b.selected.saturating_sub(1),
            Key::PageUp => b.selected = b.selected.saturating_sub(g.list_h),
            Key::PageDown => b.selected = (b.selected + g.list_h).min(last),
            Key::Enter => {
                if let Some(o) = hits.get(b.selected) {
                    self.activate_option(b, o.key);
                }
            }
            Key::Char('r') if k.mods.ctrl => {
                if let Some(o) = hits.get(b.selected) {
                    self.settings_mut().reset(o.key);
                    self.apply_settings();
                    b.message = format!("{} reset to {}", o.key, o.default_value().display());
                }
            }
            _ => {
                if self_field_event(&mut b.query, k) {
                    b.selected = 0;
                    b.scroll = 0;
                }
            }
        }
        // Keep the selection in view.
        let n = search(&b.query.string()).len();
        b.selected = b.selected.min(n.saturating_sub(1));
        if b.selected < b.scroll {
            b.scroll = b.selected;
        }
        if b.selected >= b.scroll + g.list_h {
            b.scroll = b.selected + 1 - g.list_h;
        }
        true
    }

    /// Enter on an option: flip a boolean, step through a choice, or start typing a value.
    pub(crate) fn activate_option(&mut self, b: &mut SettingsBox, key: &'static str) {
        let Some(o) = option(key) else { return };
        let cur = self
            .settings()
            .get(key)
            .cloned()
            .unwrap_or_else(|| o.default_value());
        match o.kind {
            Kind::Bool => {
                let v = Value::Bool(!cur.as_bool().unwrap_or(false));
                self.set_option(b, key, v);
            }
            Kind::Choice(cs) => {
                let v = Value::str(next_choice(cs, cur.as_str().unwrap_or("")));
                self.set_option(b, key, v);
            }
            _ => {
                let text = match &cur {
                    Value::List(l) => l
                        .iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    Value::Str(s) => s.clone(),
                    other => other.display(),
                };
                b.edit = Some((key, Field::with(&text)));
                b.message = "type a value, enter to apply, esc to cancel".to_string();
            }
        }
    }

    fn commit_option(&mut self, b: &mut SettingsBox, key: &'static str, text: &str) {
        match option(key).map(|o| o.parse_text(text)) {
            Some(Ok(v)) => self.set_option(b, key, v),
            Some(Err(e)) => b.message = e,
            None => b.message = format!("unknown option {}", key),
        }
    }

    fn set_option(&mut self, b: &mut SettingsBox, key: &str, v: Value) {
        let shown = v.display();
        match self.settings_mut().set(key, v) {
            Ok(()) => {
                self.apply_settings();
                b.message = format!("{} = {}  (this session; save to keep)", key, shown);
            }
            Err(e) => b.message = e,
        }
    }

    pub(crate) fn overlay_click(&mut self, col: usize, row: usize) {
        let Some(ov) = self.overlay.take() else {
            return;
        };
        match ov {
            Overlay::Open(f) => {
                let g = prompt_geometry(self.cols, self.rows);
                if col >= g.0 && col < g.0 + g.2 && row >= g.1 && row < g.1 + 3 {
                    self.overlay = Some(Overlay::Open(f));
                }
            }
            Overlay::Settings(mut b) => {
                let g = box_geometry(self.cols, self.rows);
                let inside = col >= g.x && col < g.x + g.w && row >= g.y && row < g.y + g.h;
                if !inside {
                    return;
                }
                if row >= g.list_y && row < g.list_y + g.list_h && b.edit.is_none() {
                    let hits = search(&b.query.string());
                    let idx = b.scroll + (row - g.list_y);
                    if let Some(o) = hits.get(idx) {
                        b.selected = idx;
                        self.activate_option(&mut b, o.key);
                    }
                }
                self.overlay = Some(Overlay::Settings(b));
            }
        }
    }

    pub(crate) fn overlay_wheel(&mut self, delta: isize) {
        if let Some(Overlay::Settings(b)) = &mut self.overlay {
            let n = search(&b.query.string()).len();
            let g = box_geometry(self.cols, self.rows);
            let max_scroll = n.saturating_sub(g.list_h);
            b.scroll = (b.scroll as isize + delta).clamp(0, max_scroll as isize) as usize;
        }
    }

    pub(crate) fn paint_overlay(&self, s: &mut Screen) {
        match &self.overlay {
            None => {}
            Some(Overlay::Open(f)) => {
                let (x, y, w) = prompt_geometry(s.cols, s.rows);
                frame(s, x, y, w, 3, " open ");
                let inner = w.saturating_sub(2);
                let label = "address: ";
                s.text(x + 1, y + 1, label, Look::PLAIN, inner);
                let room = inner.saturating_sub(label.len());
                paint_field(s, x + 1 + label.len(), y + 1, room, f);
            }
            Some(Overlay::Settings(b)) => self.paint_settings(s, b),
        }
    }

    fn paint_settings(&self, s: &mut Screen, b: &SettingsBox) {
        let g = box_geometry(s.cols, s.rows);
        frame(s, g.x, g.y, g.w, g.h, " settings ");
        let inner_x = g.x + 1;
        let inner_w = g.w.saturating_sub(2);
        // search / edit line
        match &b.edit {
            Some((key, f)) => {
                let label = format!("{}: ", key);
                s.text(inner_x, g.y + 1, &label, Look::BOLD, inner_w);
                let used = label.chars().count().min(inner_w);
                paint_field(s, inner_x + used, g.y + 1, inner_w.saturating_sub(used), f);
            }
            None => {
                let label = "search: ";
                s.text(inner_x, g.y + 1, label, Look::BOLD, inner_w);
                paint_field(
                    s,
                    inner_x + label.len(),
                    g.y + 1,
                    inner_w.saturating_sub(label.len()),
                    &b.query,
                );
            }
        }
        rule(s, g.x, g.y + 2, g.w);
        let hits = search(&b.query.string());
        if hits.is_empty() {
            s.text(
                inner_x + 1,
                g.list_y,
                "no matching options",
                Look::PLAIN,
                inner_w,
            );
        }
        for i in 0..g.list_h {
            let idx = b.scroll + i;
            let Some(o) = hits.get(idx) else { break };
            let cur = self
                .settings()
                .get(o.key)
                .cloned()
                .unwrap_or_else(|| o.default_value());
            let changed = cur != o.default_value();
            let line = format!(
                "{} {} = {}{}",
                if idx == b.selected { '>' } else { ' ' },
                o.key,
                cur.display(),
                if changed { "  *" } else { "" }
            );
            let look = if idx == b.selected && b.edit.is_none() {
                Look::REVERSE
            } else {
                Look::PLAIN
            };
            let y = g.list_y + i;
            s.fill(inner_x, y, inner_w, 1, look);
            s.text(inner_x, y, &clip(&line, inner_w), look, inner_w);
        }
        let rule_y = g.list_y + g.list_h;
        rule(s, g.x, rule_y, g.w);
        let desc = hits.get(b.selected).map_or("", |o| o.desc);
        s.text(
            inner_x,
            rule_y + 1,
            &clip(desc, inner_w),
            Look::PLAIN,
            inner_w,
        );
        let help = if !b.message.is_empty() {
            b.message.clone()
        } else {
            let save = self
                .keymap
                .first_label(Action::SaveSettings)
                .unwrap_or_else(|| "(unbound)".into());
            format!(
                "enter change  ctrl-r reset  {} save to profile  esc close  (* = changed)",
                save
            )
        };
        s.text(
            inner_x,
            rule_y + 2,
            &clip(&help, inner_w),
            Look::BOLD,
            inner_w,
        );
    }

    /// Tells the app something it must do (clipboard, a new window, quitting).
    pub(crate) fn effect(&mut self, e: Effect) {
        self.effects.push(e);
    }
}

fn prompt_geometry(cols: usize, rows: usize) -> (usize, usize, usize) {
    let w = cols.saturating_sub(4).clamp(1, 78).min(cols);
    let x = (cols - w) / 2;
    let y = rows.saturating_sub(3) / 2;
    (x, y, w)
}

/// Typing into the search field; true when its text changed.
fn self_field_event(f: &mut Field, k: KeyPress) -> bool {
    matches!(f.key(k), FieldEvent::Changed)
}

fn frame(s: &mut Screen, x: usize, y: usize, w: usize, h: usize, title: &str) {
    if w < 2 || h < 2 {
        return;
    }
    // Shapes of the page and the tabs must not show through the box.
    s.cover(x, y, w, h);
    s.fill(x, y, w, h, Look::PLAIN);
    let put = |s: &mut Screen, cx: usize, cy: usize, ch: char| {
        s.set(
            cx,
            cy,
            Cell {
                ch,
                ..Cell::default()
            },
        );
    };
    put(s, x, y, '┌');
    put(s, x + w - 1, y, '┐');
    put(s, x, y + h - 1, '└');
    put(s, x + w - 1, y + h - 1, '┘');
    for cx in x + 1..x + w - 1 {
        put(s, cx, y, '─');
        put(s, cx, y + h - 1, '─');
    }
    for cy in y + 1..y + h - 1 {
        put(s, x, cy, '│');
        put(s, x + w - 1, cy, '│');
    }
    s.text(
        x + 2,
        y,
        &clip(title, w.saturating_sub(4)),
        Look::BOLD,
        w.saturating_sub(4),
    );
}

fn rule(s: &mut Screen, x: usize, y: usize, w: usize) {
    if w < 2 {
        return;
    }
    s.set(
        x,
        y,
        Cell {
            ch: '├',
            ..Cell::default()
        },
    );
    s.set(
        x + w - 1,
        y,
        Cell {
            ch: '┤',
            ..Cell::default()
        },
    );
    for cx in x + 1..x + w - 1 {
        s.set(
            cx,
            y,
            Cell {
                ch: '─',
                ..Cell::default()
            },
        );
    }
}

/// Draws a field in `room` cells, scrolled so the caret is visible; the caret cell is reversed.
fn paint_field(s: &mut Screen, x: usize, y: usize, room: usize, f: &Field) {
    if room == 0 {
        return;
    }
    let start = (f.caret + 1).saturating_sub(room);
    for i in 0..room {
        let idx = start + i;
        let ch = f.text.get(idx).copied().unwrap_or(' ');
        let reverse = idx == f.caret;
        s.set(
            x + i,
            y,
            Cell {
                ch,
                reverse,
                ..Cell::default()
            },
        );
    }
}
