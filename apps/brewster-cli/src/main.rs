//! brewster-cli: terminal viewer. All parsing/layout lives in `crates/*`;
//! this app only reads input and paints rows with ANSI escapes.

use engine::{Rgb, Row, render_source, row_text};
use std::env;
use std::fs;
use std::io::{self, Read};

const USAGE: &str =
    "usage: brewster-cli [--width N] [--plain] [FILE]\n(reads stdin when FILE is omitted)";

fn paint(row: &Row) -> String {
    let mut out = String::new();
    for s in row {
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

fn run() -> Result<(), String> {
    let mut width = 80usize;
    let mut path: Option<String> = None;
    let mut plain = env::var_os("NO_COLOR").is_some();
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--width" => {
                let v = args.next().ok_or("--width needs a number")?;
                width = v
                    .parse()
                    .map_err(|_| "--width needs a number".to_string())?;
            }
            "--plain" => plain = true,
            "-h" | "--help" => {
                println!("{}", USAGE);
                return Ok(());
            }
            _ => path = Some(a.clone()),
        }
    }

    let src = match path {
        Some(p) => fs::read_to_string(&p).map_err(|e| format!("cannot read {}: {}", p, e))?,
        None => {
            let mut s = String::new();
            io::stdin()
                .read_to_string(&mut s)
                .map_err(|e| e.to_string())?;
            s
        }
    };

    let rows = render_source(&src, width)?;
    let mut out = String::new();
    for row in &rows {
        if plain {
            out.push_str(&row_text(row));
        } else {
            out.push_str(&paint(row));
        }
        out.push('\n');
    }
    print!("{}", out);
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
