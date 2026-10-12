//! A nested key/value tree: the data model shared by settings and profiles, and by the
//! TOML, JSON and YAML readers and writers.

use std::collections::BTreeMap;

pub type Map = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    Map(Map),
}

impl Value {
    pub fn map() -> Value {
        Value::Map(Map::new())
    }

    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(s.into())
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&Map> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_map_mut(&mut self) -> Option<&mut Map> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Looks up a dotted path (`tabs.position`). Keys that contain a dot cannot be reached.
    pub fn get(&self, path: &str) -> Option<&Value> {
        let mut v = self;
        for part in path.split('.') {
            v = v.as_map()?.get(part)?;
        }
        Some(v)
    }

    /// Sets a dotted path, creating the maps on the way.
    pub fn set(&mut self, path: &str, value: Value) -> Result<(), String> {
        if path.is_empty() || path.split('.').any(str::is_empty) {
            return Err(format!("`{}` is not a valid key", path));
        }
        let mut cur = self;
        let parts: Vec<&str> = path.split('.').collect();
        for (i, part) in parts.iter().enumerate() {
            let Value::Map(m) = cur else {
                return Err(format!("`{}` is not a table", parts[..i].join(".")));
            };
            if i + 1 == parts.len() {
                m.insert(part.to_string(), value);
                return Ok(());
            }
            cur = m.entry(part.to_string()).or_insert_with(Value::map);
        }
        Ok(())
    }

    /// Removes a dotted path; returns the old value.
    pub fn remove(&mut self, path: &str) -> Option<Value> {
        let (parent, last) = match path.rfind('.') {
            Some(i) => (Some(&path[..i]), &path[i + 1..]),
            None => (None, path),
        };
        let m = match parent {
            Some(p) => self.get_mut(p)?.as_map_mut()?,
            None => self.as_map_mut()?,
        };
        m.remove(last)
    }

    fn get_mut(&mut self, path: &str) -> Option<&mut Value> {
        let mut v = self;
        for part in path.split('.') {
            v = v.as_map_mut()?.get_mut(part)?;
        }
        Some(v)
    }

    /// Every leaf (anything that is not a non-empty map) with its dotted path, in key order.
    pub fn flatten(&self) -> Vec<(String, &Value)> {
        let mut out = Vec::new();
        fn walk<'a>(prefix: &str, v: &'a Value, out: &mut Vec<(String, &'a Value)>) {
            match v {
                Value::Map(m) if !m.is_empty() => {
                    for (k, c) in m {
                        let p = if prefix.is_empty() { k.clone() } else { format!("{}.{}", prefix, k) };
                        walk(&p, c, out);
                    }
                }
                _ => out.push((prefix.to_string(), v)),
            }
        }
        walk("", self, &mut out);
        out
    }

    /// Writes `over` on top of `self`: maps merge key by key, everything else is replaced.
    pub fn merge(&mut self, over: Value) {
        match (self, over) {
            (Value::Map(a), Value::Map(b)) => {
                for (k, v) in b {
                    match a.get_mut(&k) {
                        Some(slot) => slot.merge(v),
                        None => {
                            a.insert(k, v);
                        }
                    }
                }
            }
            (slot, v) => *slot = v,
        }
    }

    /// A short single-line rendering for lists and status messages.
    pub fn display(&self) -> String {
        match self {
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => float_text(*f),
            Value::Str(s) => s.clone(),
            Value::List(l) => {
                let items: Vec<String> = l.iter().map(Value::display).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Map(m) => {
                let items: Vec<String> = m.iter().map(|(k, v)| format!("{}: {}", k, v.display())).collect();
                format!("{{{}}}", items.join(", "))
            }
        }
    }
}

/// A float that reads back as a float (`1` becomes `1.0`).
pub fn float_text(f: f64) -> String {
    if !f.is_finite() {
        return "0.0".to_string();
    }
    let s = format!("{}", f);
    if s.contains('.') || s.contains('e') || s.contains('E') { s } else { format!("{}.0", s) }
}

/// Escapes a string for a double-quoted JSON / TOML / YAML scalar.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A tiny char cursor shared by the three parsers.
pub(crate) struct Chars {
    pub c: Vec<char>,
    pub i: usize,
    pub line: usize,
}

impl Chars {
    pub fn new(s: &str) -> Chars {
        Chars { c: s.chars().collect(), i: 0, line: 1 }
    }
    pub fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }
    pub fn peek_at(&self, n: usize) -> Option<char> {
        self.c.get(self.i + n).copied()
    }
    pub fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if let Some(ch) = c {
            self.i += 1;
            if ch == '\n' {
                self.line += 1;
            }
        }
        c
    }
    pub fn eat(&mut self, ch: char) -> bool {
        if self.peek() == Some(ch) {
            self.next();
            true
        } else {
            false
        }
    }
    pub fn err<T>(&self, msg: &str) -> Result<T, String> {
        Err(format!("line {}: {}", self.line, msg))
    }
    /// Reads the body of a quoted string after the opening `"` (backslash escapes).
    pub fn basic_string(&mut self) -> Result<String, String> {
        let mut out = String::new();
        loop {
            match self.next() {
                None => return self.err("unterminated string"),
                Some('"') => return Ok(out),
                Some('\n') => return self.err("a string cannot contain a newline (use \\n)"),
                Some('\\') => match self.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('b') => out.push('\u{8}'),
                    Some('f') => out.push('\u{c}'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('/') => out.push('/'),
                    Some('u') => {
                        let hi = self.hex4()?;
                        if (0xD800..0xDC00).contains(&hi) {
                            if self.next() != Some('\\') || self.next() != Some('u') {
                                return self.err("a high surrogate needs a low surrogate");
                            }
                            let lo = self.hex4()?;
                            if !(0xDC00..0xE000).contains(&lo) {
                                return self.err("bad surrogate pair");
                            }
                            let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                            out.push(char::from_u32(cp).ok_or("bad code point")?);
                        } else {
                            match char::from_u32(hi) {
                                Some(c) => out.push(c),
                                None => return self.err("bad \\u escape"),
                            }
                        }
                    }
                    _ => return self.err("unknown escape in a string"),
                },
                Some(c) => out.push(c),
            }
        }
    }
    fn hex4(&mut self) -> Result<u32, String> {
        let mut n = 0u32;
        for _ in 0..4 {
            let d = self.next().and_then(|c| c.to_digit(16));
            match d {
                Some(d) => n = n * 16 + d,
                None => return self.err("\\u needs four hex digits"),
            }
        }
        Ok(n)
    }
}

pub(crate) const MAX_DEPTH: usize = 64;
