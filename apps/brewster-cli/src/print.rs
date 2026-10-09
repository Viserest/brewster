//! One-shot output: render once and print to stdout (ANSI colors or plain text).

use engine::{Rgb, Row, row_text};

fn paint(row: &Row) -> String {
    let mut out = String::new();
    for s in &row.spans {
        if s.text.is_empty() {
            continue;
        }
        let mut codes: Vec<String> = Vec::new();
        if s.bold {
            codes.push("1".to_string());
        }
        if s.underline {
            codes.push("4".to_string());
        }
        if let Some(Rgb(r, g, b)) = s.fg {
            codes.push(format!("38;2;{};{};{}", r, g, b));
        }
        if let Some(Rgb(r, g, b)) = s.bg {
            codes.push(format!("48;2;{};{};{}", r, g, b));
        }
        if codes.is_empty() {
            out.push_str(&s.text);
        } else {
            out.push_str(&format!("\x1b[{}m{}\x1b[0m", codes.join(";"), s.text));
        }
    }
    out
}

pub fn render(rows: &[Row], plain: bool) -> String {
    let mut out = String::new();
    for row in rows {
        if plain {
            out.push_str(&row_text(row));
        } else {
            out.push_str(&paint(row));
        }
        out.push('\n');
    }
    out
}
