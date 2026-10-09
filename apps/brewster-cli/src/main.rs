//! brewster-cli: terminal viewer. All parsing/layout lives in `crates/*`;
//! this app only handles input, output and terminal interaction.
//!
//! - `brewster-cli FILE` on a terminal opens the interactive viewer.
//! - `--once`, `--plain`, piped output, or stdin input print once and exit.

mod app;
mod clipboard;
mod print;
mod tui;
mod view;

use engine::render_source;
use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read};

const USAGE: &str = "usage: brewster-cli [--once] [--plain] [--width N] [FILE]\n\
  FILE on a terminal opens the interactive viewer\n\
  (j/k/Tab move the cursor, m cursor mode, y copy, enter edit input,\n\
   p/ctrl-v paste, space/b page, g/G first/last, r reload, q quit)\n\
  --once    print once with colors and exit\n\
  --plain   print once without colors and exit\n\
  --width N layout width for one-shot output (default 80)\n\
  stdin is read when FILE is omitted (one-shot)";

fn run() -> Result<(), String> {
    let mut width = 80usize;
    let mut path: Option<String> = None;
    let mut plain = env::var_os("NO_COLOR").is_some();
    let mut once = false;
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--width" => {
                let v = args.next().ok_or("--width needs a number")?;
                width = v
                    .parse()
                    .map_err(|_| "--width needs a number".to_string())?;
            }
            "--plain" => {
                plain = true;
                once = true;
            }
            "--once" => once = true,
            "-h" | "--help" => {
                println!("{}", USAGE);
                return Ok(());
            }
            _ => path = Some(a.clone()),
        }
    }

    if let Some(p) = &path {
        if !once && io::stdout().is_terminal() {
            return tui::run(p);
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
    print!("{}", print::render(&rows, plain));
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
