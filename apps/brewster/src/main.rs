//! brewster: GUI viewer. All parsing, layout, tabs, settings and profile logic lives in
//! `crates/*` (see the `browser` crate); this app only provides a window.
//!
//! The windowing backend is a Cargo feature. `egui` (the default) is the only one
//! implemented; `iced` and `slint` are reserved names and fail to compile for now.
//!
//! ```text
//! cargo run -p brewster -- examples/demo.rs
//! ```

mod grid;

// :: Backend to use
// Egui
#[cfg(feature = "egui")]
mod egui_backend;
// Iced
#[cfg(feature = "iced")]
compile_error!("the `iced` backend is not implemented yet; use the default `egui` feature");
// Slint
#[cfg(feature = "slint")]
compile_error!("the `slint` backend is not implemented yet; use the default `egui` feature");

// None selected
#[cfg(not(any(feature = "egui", feature = "iced", feature = "slint")))]
compile_error!("no GUI backend selected; enable the `egui` feature (it is on by default)");

use browser::launch::{OPTIONS_HELP, Options};

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


const USAGE: &str = r#"usage: brewster [OPTIONS] [FILE|URL]...
  Opens the files and addresses as tabs in a window. Without any, the last session's
  tabs (or the home page) open. The page is as big as the window.
  ctrl-p settings   ctrl-t new tab   ctrl-w close tab   alt-right/left switch tab
  ctrl-l open address   enter or click opens a link (ctrl-click: new tab, shift-click:
  new window)   q quit   (all keys can be changed in the settings)
Options:
{}"#;

fn run_backend(opts: Options) -> Result<(), String> {
    #[cfg(feature = "egui")]
    {
        egui_backend::run(opts)
    }
    #[cfg(not(feature = "egui"))]
    {
        let _ = opts;
        Err("no GUI backend available".to_string())
    }
}

fn run() -> Result<(), String> {
    let opts = Options::parse(std::env::args().skip(1))?;
    if opts.help {
        println!("{}", USAGE.replace("{}", OPTIONS_HELP));
        return Ok(());
    }
    run_backend(opts)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
