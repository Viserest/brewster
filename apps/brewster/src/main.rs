//! brewster: GUI viewer. All parsing, layout, cursor and editing logic lives in
//! `crates/*` (see the `viewer` crate); this app only provides a window.
//!
//! The windowing backend is a Cargo feature. `egui` (the default) is the only one
//! implemented; `iced` and `slint` are reserved names and fail to compile for now.
//!
//! ```text
//! cargo run -p brewster -- examples/demo.cre
//! ```

#[cfg(feature = "iced")]
compile_error!("the `iced` backend is not implemented yet; use the default `egui` feature");
#[cfg(feature = "slint")]
compile_error!("the `slint` backend is not implemented yet; use the default `egui` feature");
#[cfg(not(any(feature = "egui", feature = "iced", feature = "slint")))]
compile_error!("no GUI backend selected; enable the `egui` feature (it is on by default)");

mod grid;

#[cfg(feature = "egui")]
mod egui_backend;

const USAGE: &str = "usage: brewster FILE\n\
  opens FILE in a window\n\
  (j/k/Tab move the cursor, m cursor mode, y copy, enter edit input,\n\
   ctrl-v paste, mouse wheel / space / b scroll, r reload, q quit)";

fn run_backend(path: &str) -> Result<(), String> {
    #[cfg(feature = "egui")]
    {
        egui_backend::run(path)
    }
    #[cfg(not(feature = "egui"))]
    {
        let _ = path;
        Err("no GUI backend available".to_string())
    }
}

fn run() -> Result<(), String> {
    let mut path: Option<String> = None;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{}", USAGE);
                return Ok(());
            }
            _ if a.starts_with('-') => return Err(format!("unknown option `{}`\n{}", a, USAGE)),
            _ => path = Some(a),
        }
    }
    match path {
        Some(p) => run_backend(&p),
        None => Err(USAGE.to_string()),
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
