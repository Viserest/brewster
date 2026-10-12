//! TOML reader and writer (the subset settings need: tables, dotted keys, strings, numbers,
//! booleans, arrays and inline tables; no dates, no multi-line strings, no `[[arrays]]`).

use crate::value::{Chars, MAX_DEPTH, Map, Value, escape, float_text};

use std::collections::HashSet;

fn is_bare(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn skip_ws(p: &mut Chars) {
    while matches!(p.peek(), Some(' ' | '\t')) {
        p.next();
    }
}

/// Whitespace, comments and newlines (inside arrays).
fn skip_all(p: &mut Chars) {
    loop {
        match p.peek() {
            Some(' ' | '\t' | '\n' | '\r') => {
                p.next();
            }
            Some('#') => {
                while !matches!(p.peek(), None | Some('\n')) {
                    p.next();
                }
            }
            _ => return,
        }
    }
}

fn end_of_line(p: &mut Chars) -> Result<(), String> {
    skip_ws(p);
    if p.peek() == Some('#') {
        while !matches!(p.peek(), None | Some('\n')) {
            p.next();
        }
    }
    match p.peek() {
        None => Ok(()),
        Some('\n') => {
            p.next();
            Ok(())
        }
        Some('\r') if p.peek_at(1) == Some('\n') => {
            p.next();
            p.next();
            Ok(())
        }
        Some(_) => p.err("unexpected text after the value"),
    }
}

fn key_part(p: &mut Chars) -> Result<String, String> {
    match p.peek() {
        Some('"') => {
            p.next();
            p.basic_string()
        }
        Some('\'') => {
            p.next();
            literal_string(p)
        }
        Some(c) if is_bare(c) => {
            let start = p.i;
            while matches!(p.peek(), Some(c) if is_bare(c)) {
                p.next();
            }
            Ok(p.c[start..p.i].iter().collect())
        }
        _ => p.err("expected a key"),
    }
}

fn key_path(p: &mut Chars) -> Result<Vec<String>, String> {
    let mut path = vec![key_part(p)?];
    loop {
        skip_ws(p);
        if p.peek() == Some('.') {
            p.next();
            skip_ws(p);
            path.push(key_part(p)?);
        } else {
            return Ok(path);
        }
    }
}

fn literal_string(p: &mut Chars) -> Result<String, String> {
    let mut s = String::new();
    loop {
        match p.next() {
            None | Some('\n') => return p.err("unterminated string"),
            Some('\'') => return Ok(s),
            Some(c) => s.push(c),
        }
    }
}

fn value(p: &mut Chars, depth: usize) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return p.err("nested too deeply");
    }
    match p.peek() {
        Some('"') => {
            if p.peek_at(1) == Some('"') && p.peek_at(2) == Some('"') {
                return p.err("multi-line strings are not supported");
            }
            p.next();
            Ok(Value::Str(p.basic_string()?))
        }
        Some('\'') => {
            p.next();
            Ok(Value::Str(literal_string(p)?))
        }
        Some('[') => {
            p.next();
            let mut l = Vec::new();
            loop {
                skip_all(p);
                if p.eat(']') {
                    return Ok(Value::List(l));
                }
                l.push(value(p, depth + 1)?);
                skip_all(p);
                if p.eat(',') {
                    continue;
                }
                if p.eat(']') {
                    return Ok(Value::List(l));
                }
                return p.err("expected `,` or `]` in the array");
            }
        }
        Some('{') => {
            p.next();
            let mut root = Value::map();
            skip_ws(p);
            if p.eat('}') {
                return Ok(root);
            }
            loop {
                skip_ws(p);
                let path = key_path(p)?;
                skip_ws(p);
                if !p.eat('=') {
                    return p.err("expected `=` after the key");
                }
                skip_ws(p);
                let v = value(p, depth + 1)?;
                insert(&mut root, &path, v, p)?;
                skip_ws(p);
                if p.eat(',') {
                    continue;
                }
                if p.eat('}') {
                    return Ok(root);
                }
                return p.err("expected `,` or `}` in the inline table");
            }
        }
        Some(_) => {
            let start = p.i;
            while matches!(p.peek(), Some(c) if !matches!(c, ',' | ']' | '}' | '#' | ' ' | '\t' | '\n' | '\r'))
            {
                p.next();
            }
            let t: String = p.c[start..p.i].iter().collect();
            match t.as_str() {
                "true" => return Ok(Value::Bool(true)),
                "false" => return Ok(Value::Bool(false)),
                "" => return p.err("expected a value"),
                _ => {}
            }
            let clean = t.replace('_', "");
            if let Ok(i) = clean.parse::<i64>() {
                return Ok(Value::Int(i));
            }
            if clean.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+') {
                if let Ok(f) = clean.parse::<f64>() {
                    if f.is_finite() {
                        return Ok(Value::Float(f));
                    }
                }
            }
            p.err(&format!("bad value `{}` (strings need quotes)", t))
        }
        None => p.err("expected a value"),
    }
}

fn insert(root: &mut Value, path: &[String], v: Value, p: &Chars) -> Result<(), String> {
    let mut cur = root;
    for (i, part) in path.iter().enumerate() {
        let Value::Map(m) = cur else {
            return p.err(&format!("`{}` is not a table", path[..i].join(".")));
        };
        if i + 1 == path.len() {
            if m.contains_key(part) {
                return p.err(&format!("duplicate key `{}`", path.join(".")));
            }
            m.insert(part.clone(), v);
            return Ok(());
        }
        cur = m.entry(part.clone()).or_insert_with(Value::map);
    }
    Ok(())
}

pub fn parse(src: &str) -> Result<Value, String> {
    let mut p = Chars::new(src);
    let mut root = Value::map();
    let mut table: Vec<String> = Vec::new();
    let mut headers: HashSet<Vec<String>> = HashSet::new();
    loop {
        skip_all(&mut p);
        let Some(c) = p.peek() else { return Ok(root) };
        if c == '[' {
            p.next();
            if p.peek() == Some('[') {
                return p.err("arrays of tables (`[[x]]`) are not supported");
            }
            skip_ws(&mut p);
            let path = key_path(&mut p)?;
            skip_ws(&mut p);
            if !p.eat(']') {
                return p.err("expected `]` to close the table header");
            }
            end_of_line(&mut p)?;
            if !headers.insert(path.clone()) {
                return p.err(&format!("table `{}` is defined twice", path.join(".")));
            }
            // Make sure the table exists even when it stays empty.
            let mut cur = &mut root;
            for part in &path {
                let Value::Map(m) = cur else {
                    return p.err(&format!("`{}` is not a table", path.join(".")));
                };
                cur = m.entry(part.clone()).or_insert_with(Value::map);
            }
            if !matches!(cur, Value::Map(_)) {
                return p.err(&format!("`{}` is not a table", path.join(".")));
            }
            table = path;
        } else {
            let path = key_path(&mut p)?;
            skip_ws(&mut p);
            if !p.eat('=') {
                return p.err("expected `=` after the key");
            }
            skip_ws(&mut p);
            let v = value(&mut p, 0)?;
            end_of_line(&mut p)?;
            let mut full = table.clone();
            full.extend(path);
            insert(&mut root, &full, v, &p)?;
        }
    }
}

fn key_text(k: &str) -> String {
    if !k.is_empty() && k.chars().all(is_bare) {
        k.to_string()
    } else {
        escape(k)
    }
}

fn scalar(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("\"\""),
        Value::Bool(b) => out.push_str(&b.to_string()),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => out.push_str(&float_text(*f)),
        Value::Str(s) => out.push_str(&escape(s)),
        Value::List(l) => {
            out.push('[');
            let mut first = true;
            for x in l.iter().filter(|x| !matches!(x, Value::Null)) {
                if !first {
                    out.push_str(", ");
                }
                first = false;
                scalar(x, out);
            }
            out.push(']');
        }
        Value::Map(m) => {
            out.push('{');
            let mut first = true;
            for (k, x) in m.iter().filter(|(_, x)| !matches!(x, Value::Null)) {
                if !first {
                    out.push_str(", ");
                }
                first = false;
                out.push_str(&key_text(k));
                out.push_str(" = ");
                scalar(x, out);
            }
            out.push('}');
        }
    }
}

/// Writes a map as TOML. `null` has no TOML form, so null entries are left out.
pub fn write(v: &Value) -> String {
    let mut out = String::new();
    if let Value::Map(m) = v {
        table(m, &mut Vec::new(), &mut out);
    }
    out
}

fn table(m: &Map, path: &mut Vec<String>, out: &mut String) {
    let mut wrote_scalar = false;
    for (k, v) in m {
        if matches!(v, Value::Map(_) | Value::Null) {
            continue;
        }
        out.push_str(&key_text(k));
        out.push_str(" = ");
        scalar(v, out);
        out.push('\n');
        wrote_scalar = true;
    }
    for (k, v) in m {
        let Value::Map(sub) = v else { continue };
        path.push(key_text(k));
        let has_scalar = sub
            .values()
            .any(|x| !matches!(x, Value::Map(_) | Value::Null));
        if has_scalar || sub.is_empty() {
            if wrote_scalar || !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{}]\n", path.join(".")));
            wrote_scalar = true;
        }
        table(sub, path, out);
        path.pop();
    }
}
