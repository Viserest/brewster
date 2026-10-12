//! Key bindings: `ctrl-t`, `alt-right`, `f1`, `shift-enter`, `click`, `ctrl-click` ...
//! parsed from the `keybinds.*` settings.

use settings::Settings;
use viewer::{Key, KeyPress, Mods};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Settings,
    NewTab,
    CloseTab,
    NextTab,
    PrevTab,
    Reload,
    OpenUrl,
    SaveSettings,
    LinkOpen,
    LinkNewTab,
    LinkNewWindow,
}

/// The settings key and the action it triggers.
pub const ACTIONS: &[(&str, Action)] = &[
    ("keybinds.quit", Action::Quit),
    ("keybinds.settings", Action::Settings),
    ("keybinds.new_tab", Action::NewTab),
    ("keybinds.close_tab", Action::CloseTab),
    ("keybinds.next_tab", Action::NextTab),
    ("keybinds.prev_tab", Action::PrevTab),
    ("keybinds.reload", Action::Reload),
    ("keybinds.open_url", Action::OpenUrl),
    ("keybinds.save_settings", Action::SaveSettings),
    ("keybinds.link_open", Action::LinkOpen),
    ("keybinds.link_new_tab", Action::LinkNewTab),
    ("keybinds.link_new_window", Action::LinkNewWindow),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    Key(KeyPress),
    Click { button: Button, mods: Mods },
}

/// Shift is part of a typed character (`T`), so it is not compared for `Char` keys, and
/// Ctrl+letter is the same key whatever the case.
fn normalize(mut k: KeyPress) -> KeyPress {
    if let Key::Char(c) = k.key {
        k.mods.shift = false;
        if k.mods.ctrl || k.mods.alt {
            k.key = Key::Char(c.to_ascii_lowercase());
        }
    }
    k
}

pub fn parse_binding(text: &str) -> Result<Binding, String> {
    let mut rest = text.trim();
    if rest.is_empty() {
        return Err("empty key binding".to_string());
    }
    let mut mods = Mods::default();
    loop {
        let lower = rest.to_ascii_lowercase();
        let (flag, len) = if lower.starts_with("ctrl-") {
            (&mut mods.ctrl, 5)
        } else if lower.starts_with("alt-") {
            (&mut mods.alt, 4)
        } else if lower.starts_with("shift-") {
            (&mut mods.shift, 6)
        } else {
            break;
        };
        if *flag {
            return Err(format!("`{}` repeats a modifier", text));
        }
        *flag = true;
        rest = &rest[len..];
    }
    let name = rest.to_ascii_lowercase();
    match name.as_str() {
        "click" => return Ok(Binding::Click { button: Button::Left, mods }),
        "middle-click" => return Ok(Binding::Click { button: Button::Middle, mods }),
        "right-click" => return Ok(Binding::Click { button: Button::Right, mods }),
        _ => {}
    }
    let key = match name.as_str() {
        "enter" | "return" => Key::Enter,
        "esc" | "escape" => Key::Esc,
        "tab" => Key::Tab,
        "backtab" => Key::BackTab,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "space" => Key::Char(' '),
        n if n.starts_with('f') && n.len() > 1 && n[1..].chars().all(|c| c.is_ascii_digit()) => {
            match n[1..].parse::<u8>() {
                Ok(i) if (1..=12).contains(&i) => Key::F(i),
                _ => return Err(format!("`{}`: function keys go from f1 to f12", text)),
            }
        }
        _ => {
            let mut chars = rest.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Char(c),
                _ => return Err(format!("`{}` is not a key (try ctrl-t, alt-right, f5, enter, click)", text)),
            }
        }
    };
    Ok(Binding::Key(normalize(KeyPress::new(key, mods))))
}

/// All bindings in effect. Entries that fail to parse are skipped and reported.
#[derive(Debug, Clone, Default)]
pub struct Keymap {
    entries: Vec<(Action, Vec<Binding>)>,
}

impl Keymap {
    pub fn from_settings(s: &Settings) -> (Keymap, Vec<String>) {
        let mut problems = Vec::new();
        let mut entries = Vec::new();
        for (key, action) in ACTIONS {
            let mut bindings = Vec::new();
            for text in s.list(key) {
                match parse_binding(&text) {
                    Ok(b) => bindings.push(b),
                    Err(e) => problems.push(format!("{}: {}", key, e)),
                }
            }
            entries.push((*action, bindings));
        }
        (Keymap { entries }, problems)
    }

    pub fn key(&self, press: KeyPress) -> Option<Action> {
        let press = normalize(press);
        self.entries
            .iter()
            .find(|(_, bs)| bs.iter().any(|b| *b == Binding::Key(press)))
            .map(|(a, _)| *a)
    }

    pub fn click(&self, button: Button, mods: Mods) -> Option<Action> {
        // Only the chords that name a link action matter for clicks.
        self.entries
            .iter()
            .filter(|(a, _)| matches!(a, Action::LinkOpen | Action::LinkNewTab | Action::LinkNewWindow))
            .find(|(_, bs)| bs.iter().any(|b| *b == Binding::Click { button, mods }))
            .map(|(a, _)| *a)
    }

    /// Whether an action has a key chord that involves Ctrl, Alt or a function key, so it
    /// still works while text is being typed.
    pub fn works_while_typing(press: KeyPress) -> bool {
        press.mods.ctrl || press.mods.alt || matches!(press.key, Key::F(_))
    }

    /// The first binding of an action, for hints.
    pub fn first_label(&self, action: Action) -> Option<String> {
        let (_, bs) = self.entries.iter().find(|(a, _)| *a == action)?;
        bs.iter().find_map(|b| match b {
            Binding::Key(k) => Some(label(*k)),
            _ => None,
        })
    }
}

pub fn label(k: KeyPress) -> String {
    let mut s = String::new();
    if k.mods.ctrl {
        s.push_str("ctrl-");
    }
    if k.mods.alt {
        s.push_str("alt-");
    }
    if k.mods.shift {
        s.push_str("shift-");
    }
    match k.key {
        Key::Char(' ') => s.push_str("space"),
        Key::Char(c) => s.push(c),
        Key::Esc => s.push_str("esc"),
        Key::Enter => s.push_str("enter"),
        Key::Tab => s.push_str("tab"),
        Key::BackTab => s.push_str("backtab"),
        Key::Up => s.push_str("up"),
        Key::Down => s.push_str("down"),
        Key::Left => s.push_str("left"),
        Key::Right => s.push_str("right"),
        Key::Home => s.push_str("home"),
        Key::End => s.push_str("end"),
        Key::PageUp => s.push_str("pageup"),
        Key::PageDown => s.push_str("pagedown"),
        Key::Backspace => s.push_str("backspace"),
        Key::Delete => s.push_str("delete"),
        Key::F(n) => s.push_str(&format!("f{}", n)),
    }
    s
}
