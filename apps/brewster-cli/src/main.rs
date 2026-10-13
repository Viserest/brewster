//! brewster-cli: terminal viewer. All parsing/layout lives in `crates/*`;
//! this app only handles input, output and terminal interaction.
//!
//! - `brewster-cli FILE` on a terminal opens the interactive viewer.
//! - `--once`, `--plain`, piped output, or stdin input print once and exit.

mod clipboard;
mod keys;
mod print;
mod tui;

use std::{
    env,
    io::{self, IsTerminal},
    path::Path,
    rc::Rc,
};

use browser::launch::Options;
use engine::{DefaultLoader, Url, render_url};

/// The compiler and wasm engine that load pages. Without the `wasm` feature there is no
/// engine and the browser says so when a page is opened.
#[cfg(feature = "wasm")]
pub(crate) fn wasm_host() -> (engine::Host, Option<String>) {
    match host_wasmtime::host() {
        Ok(h) => (h, None),
        Err(e) => (engine::Host::detect(), Some(format!("no wasm engine: {}", e))),
    }
}

#[cfg(not(feature = "wasm"))]
pub(crate) fn wasm_host() -> (engine::Host, Option<String>) {
    (
        engine::Host::detect(),
        Some("built without the `wasm` feature: pages cannot run".to_string()),
    )
}

const USAGE: &str = r#"usage: brewster-cli [OPTIONS] [FILE|URL]...
  Opens the files and addresses as tabs in an interactive viewer. Without any, the last
  session's tabs (or the home page) open. The viewer is as big as the terminal.
  ctrl-p settings   ctrl-t new tab   ctrl-w close tab   alt-right/left switch tab
  ctrl-l open address   enter or click opens a link (ctrl-click: new tab, shift-click:
  new window, which is a tab here)   q quit   (all keys can be changed in the settings)
Options:
  --once       print the first FILE once with colors and exit
  --plain      print the first FILE once without colors and exit
  --compile    compile the first FILE (a .rs page) to a .wasm module beside it and exit
  --width N    layout width for one-shot output (default: terminal width, or 80)
{}
  One-shot output needs a FILE or URL (a .rs page or a .wasm module)"#;

fn run() -> Result<(), String> {
    let mut width: Option<usize> = None;
    let mut plain = env::var_os("NO_COLOR").is_some();
    let mut once = false;
    let mut compile = false;
    let mut rest: Vec<String> = Vec::new();
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--width" => {
                let v = args.next().ok_or("--width needs a number")?;
                width = Some(
                    v.parse()
                        .map_err(|_| "--width needs a number".to_string())?,
                );
            }
            "--plain" => {
                plain = true;
                once = true;
            }
            "--once" => once = true,
            "--compile" => compile = true,
            // Everything after `--` is an address.
            "--" => {
                rest.push(a);
                rest.extend(args.by_ref());
            }
            _ => rest.push(a),
        }
    }
    let opts = Options::parse(rest)?;
    if opts.help {
        println!("{}", USAGE.replace("{}", browser::launch::OPTIONS_HELP));
        return Ok(());
    }

    if compile {
        let p = opts.urls.first().ok_or("--compile needs a FILE")?;
        let src = std::fs::read_to_string(p).map_err(|e| format!("cannot read {}: {}", p, e))?;
        let wasm = plugin::Compiler::detect().compile(&src)?;
        let out = Path::new(p).with_extension("wasm");
        std::fs::write(&out, wasm).map_err(|e| format!("cannot write {}: {}", out.display(), e))?;
        println!("{}", out.display());
        return Ok(());
    }

    let interactive =
        !once && io::stdout().is_terminal() && (io::stdin().is_terminal() || !opts.urls.is_empty());
    if interactive {
        return tui::run(&opts);
    }

    // One-shot: load the first page, lay it out once and print it.
    let p = opts.urls.first().ok_or("one-shot output needs a FILE or URL")?;
    let url = Url::from_input(p);
    let width = width.or_else(|| terminal_width()).unwrap_or(80);
    let (host, warning) = wasm_host();
    if let Some(w) = warning {
        eprintln!("warning: {}", w);
    }
    let rows = render_url(&url, width, Rc::new(DefaultLoader::default()), Rc::new(host))?;
    print!("{}", print::render(&rows, plain));
    Ok(())
}

/// The terminal's width when stdout is a terminal (e.g. `--once` on a terminal).
fn terminal_width() -> Option<usize> {
    if io::stdout().is_terminal() {
        crossterm::terminal::size().ok().map(|(c, _)| c as usize)
    } else {
        None
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
