//! Pages that run in this process instead of in a wasm sandbox.

use std::collections::HashMap;

use crate::{Guest, Runner};

/// A `cre` page running natively: the browser's own pages and tests.
pub struct NativeGuest(cre::LivePage);

impl NativeGuest {
    pub fn new(build: impl FnOnce() -> cre::Element) -> NativeGuest {
        NativeGuest(cre::LivePage::new(build))
    }
}

impl Guest for NativeGuest {
    fn tree(&mut self) -> Result<Vec<tree::Element>, String> {
        Ok(self.0.tree())
    }

    fn click(&mut self, id: usize) -> Result<bool, String> {
        Ok(self.0.click(id))
    }

    fn problems(&mut self) -> Vec<String> {
        self.0.problems()
    }
}

/// Runs pages that were compiled into the program. A "module" is the wasm header followed
/// by a name; [`MapRunner::module`] makes one. Tests use it with a loader that returns
/// those bytes, which exercises everything except the wasm engine itself.
#[derive(Default)]
pub struct MapRunner {
    pages: HashMap<Vec<u8>, fn() -> cre::Element>,
}

impl MapRunner {
    pub fn new() -> MapRunner {
        MapRunner::default()
    }

    /// The bytes that stand for the page `name`.
    pub fn module(name: &str) -> Vec<u8> {
        let mut b = plugin::WASM_MAGIC.to_vec();
        b.extend_from_slice(&[1, 0, 0, 0]);
        b.extend_from_slice(name.as_bytes());
        b
    }

    pub fn page(mut self, name: &str, build: fn() -> cre::Element) -> MapRunner {
        self.pages.insert(MapRunner::module(name), build);
        self
    }
}

impl Runner for MapRunner {
    fn instantiate(&self, wasm: &[u8]) -> Result<Box<dyn Guest>, String> {
        match self.pages.get(wasm) {
            Some(build) => Ok(Box::new(NativeGuest::new(*build))),
            None => Err("unknown module".to_string()),
        }
    }
}
