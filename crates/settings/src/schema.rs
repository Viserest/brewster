//! The options brewster knows: their type, default and a description. The settings search
//! box lists these; values for unknown keys are kept in the file but not offered.

use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Bool,
    Int { min: i64, max: i64 },
    Text,
    Choice(&'static [&'static str]),
    /// A list of text values (key bindings).
    List,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultValue {
    Bool(bool),
    Int(i64),
    Text(&'static str),
    List(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy)]
pub struct OptionDef {
    pub key: &'static str,
    pub kind: Kind,
    pub default: DefaultValue,
    pub desc: &'static str,
}

impl OptionDef {
    pub fn default_value(&self) -> Value {
        match self.default {
            DefaultValue::Bool(b) => Value::Bool(b),
            DefaultValue::Int(i) => Value::Int(i),
            DefaultValue::Text(s) => Value::str(s),
            DefaultValue::List(l) => Value::List(l.iter().map(|s| Value::str(*s)).collect()),
        }
    }

    /// Checks a value for this option. Text for a list option is split on commas, so the
    /// settings box can edit lists as one line.
    pub fn check(&self, v: &Value) -> Result<(), String> {
        match (self.kind, v) {
            (Kind::Bool, Value::Bool(_)) => Ok(()),
            (Kind::Int { min, max }, Value::Int(i)) if (min..=max).contains(i) => Ok(()),
            (Kind::Int { min, max }, Value::Int(_)) => Err(format!("{} must be between {} and {}", self.key, min, max)),
            (Kind::Text, Value::Str(_)) => Ok(()),
            (Kind::Choice(c), Value::Str(s)) if c.contains(&s.as_str()) => Ok(()),
            (Kind::Choice(c), Value::Str(_)) => Err(format!("{} must be one of: {}", self.key, c.join(", "))),
            (Kind::List, Value::List(l)) if l.iter().all(|x| x.as_str().is_some()) => Ok(()),
            (Kind::List, _) => Err(format!("{} must be a list of text values", self.key)),
            (Kind::Bool, _) => Err(format!("{} must be true or false", self.key)),
            (Kind::Int { .. }, _) => Err(format!("{} must be a number", self.key)),
            (_, _) => Err(format!("{} must be text", self.key)),
        }
    }

    /// Parses text typed into the settings box.
    pub fn parse_text(&self, text: &str) -> Result<Value, String> {
        let t = text.trim();
        let v = match self.kind {
            Kind::Bool => match t {
                "true" | "yes" | "on" => Value::Bool(true),
                "false" | "no" | "off" => Value::Bool(false),
                _ => return Err(format!("{} must be true or false", self.key)),
            },
            Kind::Int { .. } => Value::Int(t.parse().map_err(|_| format!("{} must be a number", self.key))?),
            Kind::Text | Kind::Choice(_) => Value::str(t),
            Kind::List => Value::List(
                t.split(',').map(str::trim).filter(|s| !s.is_empty()).map(Value::str).collect(),
            ),
        };
        self.check(&v)?;
        Ok(v)
    }
}

use DefaultValue as D;
use Kind as K;

pub const OPTIONS: &[OptionDef] = &[
    OptionDef {
        key: "general.homepage",
        kind: K::Text,
        default: D::Text("about:newtab"),
        desc: "page shown in a new tab (about:newtab, about:blank, a file or an http:// url)",
    },
    OptionDef {
        key: "general.restore_tabs",
        kind: K::Bool,
        default: D::Bool(true),
        desc: "reopen the tabs of the last session when no address is given on the command line",
    },
    OptionDef {
        key: "general.theme",
        kind: K::Choice(&["default", "chromium", "firefox", "tui"]),
        default: D::Text("default"),
        desc: "how the browser looks: default (dark and round), chromium, firefox or tui (the plain terminal look); windows draw shapes, terminals only the colors",
    },
    OptionDef {
        key: "view.cursor_mode",
        kind: K::Choice(&["focused", "all"]),
        default: D::Text("focused"),
        desc: "cursor moves over links, buttons and inputs only (focused) or over every line (all)",
    },
    OptionDef {
        key: "view.fill_window",
        kind: K::Bool,
        default: D::Bool(true),
        desc: "pages are as wide and high as the terminal or window; top-level size:max fills it",
    },
    OptionDef {
        key: "view.scale_headers",
        kind: K::Bool,
        default: D::Bool(true),
        desc: "draw header font sizes (graphical viewer only; terminals cannot scale text)",
    },
    OptionDef {
        key: "tabs.position",
        kind: K::Choice(&["top", "bottom", "left", "right"]),
        default: D::Text("top"),
        desc: "where the tab bar goes: a bar at the top or bottom, or a sidebar left or right",
    },
    OptionDef {
        key: "tabs.show",
        kind: K::Choice(&["always", "multiple", "never"]),
        default: D::Text("always"),
        desc: "show the tab bar always, only with more than one tab, or never",
    },
    OptionDef {
        key: "tabs.sidebar_width",
        kind: K::Int { min: 8, max: 80 },
        default: D::Int(24),
        desc: "width in cells of a left or right tab sidebar",
    },
    OptionDef {
        key: "tabs.title_width",
        kind: K::Int { min: 6, max: 80 },
        default: D::Int(18),
        desc: "widest tab title in a top or bottom tab bar, in cells",
    },
    OptionDef {
        key: "keybinds.quit",
        kind: K::List,
        default: D::List(&["q", "ctrl-q"]),
        desc: "quit the browser",
    },
    OptionDef {
        key: "keybinds.settings",
        kind: K::List,
        default: D::List(&["ctrl-p", "f1"]),
        desc: "open the settings search box",
    },
    OptionDef {
        key: "keybinds.new_tab",
        kind: K::List,
        default: D::List(&["ctrl-t"]),
        desc: "open a new tab",
    },
    OptionDef {
        key: "keybinds.close_tab",
        kind: K::List,
        default: D::List(&["ctrl-w"]),
        desc: "close the current tab",
    },
    OptionDef {
        key: "keybinds.next_tab",
        kind: K::List,
        default: D::List(&["alt-right", "alt-l"]),
        desc: "switch to the next tab",
    },
    OptionDef {
        key: "keybinds.prev_tab",
        kind: K::List,
        default: D::List(&["alt-left", "alt-h"]),
        desc: "switch to the previous tab",
    },
    OptionDef {
        key: "keybinds.reload",
        kind: K::List,
        default: D::List(&["r", "f5"]),
        desc: "reload the page",
    },
    OptionDef {
        key: "keybinds.open_url",
        kind: K::List,
        default: D::List(&["ctrl-l", "o"]),
        desc: "type an address or file to open in the current tab",
    },
    OptionDef {
        key: "keybinds.save_settings",
        kind: K::List,
        default: D::List(&["ctrl-s"]),
        desc: "in the settings box: save the changed settings to the profile file",
    },
    OptionDef {
        key: "keybinds.link_open",
        kind: K::List,
        default: D::List(&["click", "enter"]),
        desc: "follow the selected link in the current tab (click = left mouse button)",
    },
    OptionDef {
        key: "keybinds.link_new_tab",
        kind: K::List,
        default: D::List(&["ctrl-click", "middle-click", "ctrl-enter", "t"]),
        desc: "open the link in a new tab (terminals often cannot send ctrl-enter, so `t` works too)",
    },
    OptionDef {
        key: "keybinds.link_new_window",
        kind: K::List,
        default: D::List(&["shift-click", "shift-enter", "w"]),
        desc: "open the link in a new window (the terminal viewer has one window and opens a tab)",
    },
];

pub fn option(key: &str) -> Option<&'static OptionDef> {
    OPTIONS.iter().find(|o| o.key == key)
}

/// Options matching `query`: every word must appear in the key or the description
/// (case-insensitive). Key matches come first, then the rest in schema order.
pub fn search(query: &str) -> Vec<&'static OptionDef> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut hits: Vec<(u8, usize, &'static OptionDef)> = Vec::new();
    for (i, o) in OPTIONS.iter().enumerate() {
        let key = o.key.to_lowercase();
        let desc = o.desc.to_lowercase();
        if words.iter().all(|w| key.contains(w.as_str()) || desc.contains(w.as_str())) {
            let rank = if words.iter().all(|w| key.contains(w.as_str())) { 0 } else { 1 };
            hits.push((rank, i, o));
        }
    }
    hits.sort_by_key(|(r, i, _)| (*r, *i));
    hits.into_iter().map(|(_, _, o)| o).collect()
}
