//! YAML reader and writer for the subset settings need: block maps and lists by
//! indentation, quoted and plain scalars, and flow lists / maps (`[a, b]`, `{a: 1}`).
//! No anchors, tags, multi-line scalars or multiple documents.

use crate::value::{Chars, MAX_DEPTH, Map, Value, escape, float_text};

struct Line {
    indent: usize,
    text: String,
    no: usize,
}

/// Removes a trailing `# comment` that is outside quotes.
fn strip_comment(s: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut prev = ' ';
    for (i, c) in s.char_indices() {
        match quote {
            Some(q) => {
                if c == q && !(q == '"' && prev == '\\') {
                    quote = None;
                }
            }
            None => {
                if (c == '"' || c == '\'') && (i == 0 || prev == ' ' || matches!(prev, '[' | '{' | ',' | ':' | '-')) {
                    quote = Some(c);
                } else if c == '#' && (i == 0 || prev == ' ' || prev == '\t') {
                    return s[..i].trim_end();
                }
            }
        }
        // A backslash that is itself escaped must not escape the next quote.
        prev = if prev == '\\' && c == '\\' { ' ' } else { c };
    }
    s.trim_end()
}

fn lines(src: &str) -> Result<Vec<Line>, String> {
    let mut out = Vec::new();
    for (n, raw) in src.lines().enumerate() {
        let no = n + 1;
        if raw.contains('\t') && raw.trim_start_matches(' ').starts_with('\t') {
            return Err(format!("line {}: tabs cannot indent YAML", no));
        }
        let trimmed = strip_comment(raw);
        let body = trimmed.trim_start_matches(' ');
        if body.is_empty() {
            continue;
        }
        if out.is_empty() && body == "---" {
            continue;
        }
        if body == "---" || body == "..." {
            return Err(format!("line {}: multiple documents are not supported", no));
        }
        out.push(Line { indent: trimmed.len() - body.len(), text: body.to_string(), no });
    }
    Ok(out)
}

pub fn parse(src: &str) -> Result<Value, String> {
    let ls = lines(src)?;
    if ls.is_empty() {
        return Ok(Value::map());
    }
    let mut pos = 0;
    let v = block(&ls, &mut pos, ls[0].indent, 0)?;
    if pos < ls.len() {
        return Err(format!("line {}: unexpected indentation", ls[pos].no));
    }
    Ok(v)
}

fn is_item(text: &str) -> bool {
    text == "-" || text.starts_with("- ")
}

fn block(ls: &[Line], pos: &mut usize, indent: usize, depth: usize) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return Err(format!("line {}: nested too deeply", ls[*pos].no));
    }
    if is_item(&ls[*pos].text) {
        list(ls, pos, indent, depth)
    } else if split_key(&ls[*pos].text)?.is_some() {
        map(ls, pos, indent, depth)
    } else {
        let l = &ls[*pos];
        *pos += 1;
        scalar(&l.text, l.no)
    }
}

/// Splits `key: rest` (the colon must be followed by a space or end the line).
fn split_key(text: &str) -> Result<Option<(String, String)>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let key;
    if chars.first() == Some(&'"') || chars.first() == Some(&'\'') {
        let q = chars[0];
        let mut j = 1;
        let mut s = String::new();
        loop {
            match chars.get(j) {
                None => return Ok(None),
                Some('\\') if q == '"' => {
                    let mut ch = Chars::new(&format!("\\{}\"", chars.get(j + 1).copied().unwrap_or(' ')));
                    let t = ch.basic_string()?;
                    s.push_str(&t);
                    j += 2;
                }
                Some('\'') if q == '\'' && chars.get(j + 1) == Some(&'\'') => {
                    s.push('\'');
                    j += 2;
                }
                Some(c) if *c == q => {
                    j += 1;
                    break;
                }
                Some(c) => {
                    s.push(*c);
                    j += 1;
                }
            }
        }
        if chars.get(j) != Some(&':') {
            return Ok(None);
        }
        key = s;
        i = j;
    } else {
        if matches!(chars.first(), Some('[' | '{')) {
            return Ok(None);
        }
        let mut found = None;
        while i < chars.len() {
            if chars[i] == ':' && (i + 1 == chars.len() || chars[i + 1] == ' ') {
                found = Some(i);
                break;
            }
            i += 1;
        }
        let Some(j) = found else { return Ok(None) };
        key = chars[..j].iter().collect::<String>().trim_end().to_string();
        i = j;
    }
    let rest: String = chars[i + 1..].iter().collect();
    Ok(Some((key, rest.trim().to_string())))
}

fn map(ls: &[Line], pos: &mut usize, indent: usize, depth: usize) -> Result<Value, String> {
    let mut m = Map::new();
    while *pos < ls.len() {
        let l = &ls[*pos];
        if l.indent < indent {
            break;
        }
        if l.indent > indent {
            return Err(format!("line {}: unexpected indentation", l.no));
        }
        if is_item(&l.text) {
            break;
        }
        let Some((k, rest)) = split_key(&l.text)? else {
            return Err(format!("line {}: expected `key: value`", l.no));
        };
        let no = l.no;
        *pos += 1;
        let v = if rest.is_empty() {
            match ls.get(*pos) {
                // A list may sit at the same indent as its key.
                Some(n) if n.indent > indent || (n.indent == indent && is_item(&n.text)) => {
                    let ni = n.indent;
                    block(ls, pos, ni, depth + 1)?
                }
                _ => Value::Null,
            }
        } else {
            scalar(&rest, no)?
        };
        if m.insert(k.clone(), v).is_some() {
            return Err(format!("line {}: duplicate key `{}`", no, k));
        }
    }
    Ok(Value::Map(m))
}

fn list(ls: &[Line], pos: &mut usize, indent: usize, depth: usize) -> Result<Value, String> {
    let mut items = Vec::new();
    while *pos < ls.len() {
        let l = &ls[*pos];
        if l.indent < indent {
            break;
        }
        if l.indent > indent {
            return Err(format!("line {}: unexpected indentation", l.no));
        }
        if !is_item(&l.text) {
            break;
        }
        let rest = l.text[1..].trim_start().to_string();
        let no = l.no;
        let gap = l.text.len() - rest.len();
        if rest.is_empty() {
            *pos += 1;
            match ls.get(*pos) {
                Some(n) if n.indent > indent => {
                    let ni = n.indent;
                    items.push(block(ls, pos, ni, depth + 1)?);
                }
                _ => items.push(Value::Null),
            }
        } else if split_key(&rest)?.is_some() {
            // `- key: v`: the rest of the line starts a map indented past the dash.
            let new_indent = indent + gap;
            let first = Line { indent: new_indent, text: rest, no };
            let mut sub: Vec<Line> = vec![first];
            *pos += 1;
            while *pos < ls.len() && ls[*pos].indent > indent {
                sub.push(Line { indent: ls[*pos].indent, text: ls[*pos].text.clone(), no: ls[*pos].no });
                *pos += 1;
            }
            let mut p2 = 0;
            let v = block(&sub, &mut p2, new_indent, depth + 1)?;
            if p2 < sub.len() {
                return Err(format!("line {}: unexpected indentation", sub[p2].no));
            }
            items.push(v);
        } else if is_item(&rest) {
            return Err(format!("line {}: nested `- -` lists are not supported (use `[a, b]`)", no));
        } else {
            *pos += 1;
            items.push(scalar(&rest, no)?);
        }
    }
    Ok(Value::List(items))
}

/// A scalar or flow collection written after a key or dash.
fn scalar(text: &str, no: usize) -> Result<Value, String> {
    let t = text.trim();
    if t.starts_with(['|', '>']) {
        return Err(format!("line {}: multi-line scalars are not supported", no));
    }
    if t.starts_with(['&', '*', '!']) {
        return Err(format!("line {}: anchors, aliases and tags are not supported", no));
    }
    let mut p = Chars::new(t);
    p.line = no;
    let v = flow(&mut p, 0, false)?;
    while matches!(p.peek(), Some(' ')) {
        p.next();
    }
    if p.peek().is_some() {
        return Err(format!("line {}: unexpected text after the value", no));
    }
    Ok(v)
}

fn flow(p: &mut Chars, depth: usize, nested: bool) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return p.err("nested too deeply");
    }
    match p.peek() {
        Some('[') => {
            p.next();
            let mut l = Vec::new();
            loop {
                skip(p);
                if p.eat(']') {
                    return Ok(Value::List(l));
                }
                l.push(flow(p, depth + 1, true)?);
                skip(p);
                if p.eat(',') {
                    continue;
                }
                if p.eat(']') {
                    return Ok(Value::List(l));
                }
                return p.err("expected `,` or `]`");
            }
        }
        Some('{') => {
            p.next();
            let mut m = Map::new();
            loop {
                skip(p);
                if p.eat('}') {
                    return Ok(Value::Map(m));
                }
                let k = match flow_key(p)? {
                    Value::Str(s) => s,
                    other => other.display(),
                };
                skip(p);
                if !p.eat(':') {
                    return p.err("expected `:` after the key");
                }
                skip(p);
                let v = flow(p, depth + 1, true)?;
                if m.insert(k.clone(), v).is_some() {
                    return p.err(&format!("duplicate key `{}`", k));
                }
                skip(p);
                if p.eat(',') {
                    continue;
                }
                if p.eat('}') {
                    return Ok(Value::Map(m));
                }
                return p.err("expected `,` or `}`");
            }
        }
        Some('"') => {
            p.next();
            Ok(Value::Str(p.basic_string()?))
        }
        Some('\'') => {
            p.next();
            single_quoted(p)
        }
        _ => {
            let start = p.i;
            while let Some(c) = p.peek() {
                if nested && matches!(c, ',' | ']' | '}') {
                    break;
                }
                p.next();
            }
            let t: String = p.c[start..p.i].iter().collect();
            Ok(plain(t.trim()))
        }
    }
}

fn flow_key(p: &mut Chars) -> Result<Value, String> {
    match p.peek() {
        Some('"') => {
            p.next();
            Ok(Value::Str(p.basic_string()?))
        }
        Some('\'') => {
            p.next();
            single_quoted(p)
        }
        _ => {
            let start = p.i;
            while matches!(p.peek(), Some(c) if c != ':' && c != ',' && c != '}') {
                p.next();
            }
            let t: String = p.c[start..p.i].iter().collect();
            Ok(Value::Str(t.trim().to_string()))
        }
    }
}

fn single_quoted(p: &mut Chars) -> Result<Value, String> {
    let mut s = String::new();
    loop {
        match p.next() {
            None => return p.err("unterminated string"),
            Some('\'') if p.peek() == Some('\'') => {
                p.next();
                s.push('\'');
            }
            Some('\'') => return Ok(Value::Str(s)),
            Some(c) => s.push(c),
        }
    }
}

fn skip(p: &mut Chars) {
    while matches!(p.peek(), Some(' ' | '\t')) {
        p.next();
    }
}

/// An unquoted scalar: null, bool, number or text.
fn plain(t: &str) -> Value {
    match t {
        "" | "~" | "null" | "Null" | "NULL" => return Value::Null,
        "true" | "True" | "TRUE" => return Value::Bool(true),
        "false" | "False" | "FALSE" => return Value::Bool(false),
        _ => {}
    }
    if let Ok(i) = t.parse::<i64>() {
        return Value::Int(i);
    }
    if t.starts_with(|c: char| c.is_ascii_digit() || c == '-' || c == '+' || c == '.')
        && t.chars().any(|c| c.is_ascii_digit())
    {
        if let Ok(f) = t.parse::<f64>() {
            if f.is_finite() {
                return Value::Float(f);
            }
        }
    }
    Value::Str(t.to_string())
}

pub fn write(v: &Value) -> String {
    let mut out = String::new();
    match v {
        Value::Map(m) if !m.is_empty() => emit_map(m, 0, &mut out),
        other => {
            out.push_str(&inline(other));
            out.push('\n');
        }
    }
    out
}

fn key_text(k: &str) -> String {
    if !k.is_empty()
        && k.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && plain(k) == Value::Str(k.to_string())
        && !k.starts_with('-')
    {
        k.to_string()
    } else {
        escape(k)
    }
}

fn str_text(s: &str) -> String {
    let safe = !s.is_empty()
        && plain(s) == Value::Str(s.to_string())
        && s == s.trim()
        && !s.starts_with(['-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%', '@', '`'])
        && !s.contains(": ")
        && !s.contains(" #")
        && !s.ends_with(':')
        && !s.chars().any(|c| c.is_control());
    if safe { s.to_string() } else { escape(s) }
}

/// One-line form, used for scalars and for containers inside lists.
fn inline(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => float_text(*f),
        Value::Str(s) => str_text(s),
        Value::List(l) => format!("[{}]", l.iter().map(inline).collect::<Vec<_>>().join(", ")),
        Value::Map(m) => format!(
            "{{{}}}",
            m.iter().map(|(k, x)| format!("{}: {}", key_text(k), inline(x))).collect::<Vec<_>>().join(", ")
        ),
    }
}

fn emit_map(m: &Map, ind: usize, out: &mut String) {
    let pad = " ".repeat(ind);
    for (k, v) in m {
        match v {
            Value::Map(sub) if !sub.is_empty() => {
                out.push_str(&format!("{}{}:\n", pad, key_text(k)));
                emit_map(sub, ind + 2, out);
            }
            Value::List(l) if !l.is_empty() => {
                out.push_str(&format!("{}{}:\n", pad, key_text(k)));
                for x in l {
                    out.push_str(&format!("{}  - {}\n", pad, inline(x)));
                }
            }
            other => out.push_str(&format!("{}{}: {}\n", pad, key_text(k), inline(other))),
        }
    }
}
