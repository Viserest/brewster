//! Running page modules. A module is wasm built from a `cre` page; the browser sees only
//! its exports (see `cre::abi`). This crate holds the pieces around the runtime:
//!
//! * [`Guest`]: one running page or plugin, as the browser talks to it;
//! * [`Runner`]: turns module bytes into a [`Guest`]. A wasm engine is a `Runner` that is
//!   not part of this workspace yet (it needs a crate from the network, such as `wasmi`);
//!   [`MapRunner`] runs pages compiled into the program, for tests and tooling;
//! * [`Session`]: a page and its plugins side by side behind one tree and one click entry;
//! * [`Host`]: the compiler and the runner, handed to whatever loads pages.

mod native;
mod session;
mod wasm;

pub use native::{MapRunner, NativeGuest};
pub use session::Session;
pub use wasm::{Instance, MAX_OUTPUT, WasmGuest};

use plugin::Compiler;

/// A running page module.
pub trait Guest {
    /// The page as plain elements. Ids are the guest's own; [`Session`] makes them unique.
    fn tree(&mut self) -> Result<Vec<tree::Element>, String>;

    /// Runs the click handler of the element with this id; `false` when it has none.
    fn click(&mut self, id: usize) -> Result<bool, String>;

    /// Mistakes the page recorded since the last call (children on a leaf and the like).
    fn problems(&mut self) -> Vec<String>;
}

/// Starts modules.
pub trait Runner {
    fn instantiate(&self, wasm: &[u8]) -> Result<Box<dyn Guest>, String>;
}

/// The runner of a build without a wasm engine: every module is refused with a message
/// that says why.
pub struct Unavailable;

impl Runner for Unavailable {
    fn instantiate(&self, _wasm: &[u8]) -> Result<Box<dyn Guest>, String> {
        Err("this build has no wasm engine: pass a `host::Runner` that runs wasm to the \
             browser (see the README)"
            .to_string())
    }
}

pub struct Host {
    pub compiler: Compiler,
    pub runner: Box<dyn Runner>,
}

impl Host {
    pub fn new(compiler: Compiler, runner: Box<dyn Runner>) -> Host {
        Host { compiler, runner }
    }

    /// The compiler from the environment (`Compiler::detect`) and no wasm engine.
    pub fn detect() -> Host {
        Host::new(Compiler::detect(), Box::new(Unavailable))
    }

    pub fn with_runner(mut self, runner: Box<dyn Runner>) -> Host {
        self.runner = runner;
        self
    }
}

#[cfg(test)]
mod tests;
