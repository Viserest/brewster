//! JSON reader and writer.

use crate::value::{Chars, MAX_DEPTH, Map, Value, escape, float_text};

pub fn parse(src: &str) -> Result<Value, String> {
    let mut p = Chars::new(src);
    ws(&mut p);
    let v = value(&mut p, 0)?;
    ws(&mut p);
    if p.peek().is_some() {
        return p.err("unexpected text after the JSON value");
    }
    Ok(v)
}

fn ws(p: &mut Chars) {
    while matches!(p.peek(), Some(' ' | '\t' | '\n' | '\r')) {
        p.next();
    }
}

fn value(p: &mut Chars, depth: usize) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return p.err("nested too deeply");
    }
    match p.peek() {
        None => p.err("expected a value"),
        Some('{') => {
            p.next();
            let mut m = Map::new();
            ws(p);
            if p.eat('}') {
                return Ok(Value::Map(m));
            }
            loop {
                ws(p);
                if !p.eat('"') {
                    return p.err("expected a quoted key");
                }
                let k = p.basic_string()?;
                ws(p);
                if !p.eat(':') {
                    return p.err("expected `:` after the key");
                }
                ws(p);
                let v = value(p, depth + 1)?;
                if m.insert(k.clone(), v).is_some() {
                    return p.err(&format!("duplicate key `{}`", k));
                }
                ws(p);
                if p.eat(',') {
                    continue;
                }
                if p.eat('}') {
                    return Ok(Value::Map(m));
                }
                return p.err("expected `,` or `}`");
            }
        }
        Some('[') => {
            p.next();
            let mut l = Vec::new();
            ws(p);
            if p.eat(']') {
                return Ok(Value::List(l));
            }
            loop {
                ws(p);
                l.push(value(p, depth + 1)?);
                ws(p);
                if p.eat(',') {
                    continue;
                }
                if p.eat(']') {
                    return Ok(Value::List(l));
                }
                return p.err("expected `,` or `]`");
            }
        }
        Some('"') => {
            p.next();
            Ok(Value::Str(p.basic_string()?))
        }
        Some(c) if c == '-' || c.is_ascii_digit() => {
            let start = p.i;
            while matches!(p.peek(), Some(c) if c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')) {
                p.next();
            }
            let t: String = p.c[start..p.i].iter().collect();
            if let Ok(i) = t.parse::<i64>() {
                return Ok(Value::Int(i));
            }
            match t.parse::<f64>() {
                Ok(f) if f.is_finite() => Ok(Value::Float(f)),
                _ => p.err(&format!("bad number `{}`", t)),
            }
        }
        Some(c) if c.is_ascii_alphabetic() => {
            let start = p.i;
            while matches!(p.peek(), Some(c) if c.is_ascii_alphabetic()) {
                p.next();
            }
            let w: String = p.c[start..p.i].iter().collect();
            match w.as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                "null" => Ok(Value::Null),
                _ => p.err(&format!("unexpected `{}`", w)),
            }
        }
        Some(c) => p.err(&format!("unexpected `{}`", c)),
    }
}

pub fn write(v: &Value) -> String {
    let mut out = String::new();
    emit(v, 0, &mut out);
    out.push('\n');
    out
}

fn emit(v: &Value, ind: usize, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(&b.to_string()),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => out.push_str(&float_text(*f)),
        Value::Str(s) => out.push_str(&escape(s)),
        Value::List(l) if l.is_empty() => out.push_str("[]"),
        Value::List(l) => {
            out.push_str("[\n");
            for (i, x) in l.iter().enumerate() {
                out.push_str(&"  ".repeat(ind + 1));
                emit(x, ind + 1, out);
                out.push_str(if i + 1 < l.len() { ",\n" } else { "\n" });
            }
            out.push_str(&"  ".repeat(ind));
            out.push(']');
        }
        Value::Map(m) if m.is_empty() => out.push_str("{}"),
        Value::Map(m) => {
            out.push_str("{\n");
            let n = m.len();
            for (i, (k, x)) in m.iter().enumerate() {
                out.push_str(&"  ".repeat(ind + 1));
                out.push_str(&escape(k));
                out.push_str(": ");
                emit(x, ind + 1, out);
                out.push_str(if i + 1 < n { ",\n" } else { "\n" });
            }
            out.push_str(&"  ".repeat(ind));
            out.push('}');
        }
    }
}
