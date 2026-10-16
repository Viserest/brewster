//! Pages that run in this process instead of in a wasm sandbox.

use std::collections::HashMap;

use crate::{Guest, Runner};

/// A `cre` page running natively: the browser's own pages and tests. Like a wasm module it
/// is started when it is first asked for something, so a session can tell it the address
/// first.
pub struct NativeGuest(State);

enum State {
    Waiting(cre::Url, Box<dyn FnOnce(&cre::Root) -> cre::Element>),
    Running(cre::LivePage),
    /// While it is being started.
    Gone,
}

impl NativeGuest {
    pub fn new(build: impl FnOnce() -> cre::Element + 'static) -> NativeGuest {
        NativeGuest::at(cre::Url::default(), move |_| build())
    }

    /// A page that is built knowing its address (`root.url()`). The session's address,
    /// when it has one, replaces this one before the page is built.
    pub fn at(
        url: cre::Url,
        build: impl FnOnce(&cre::Root) -> cre::Element + 'static,
    ) -> NativeGuest {
        NativeGuest(State::Waiting(url, Box::new(build)))
    }

    fn live(&mut self) -> &cre::LivePage {
        if let State::Waiting(..) = self.0 {
            if let State::Waiting(url, build) = std::mem::replace(&mut self.0, State::Gone) {
                self.0 = State::Running(cre::LivePage::at(url, build));
            }
        }
        match &self.0 {
            State::Running(p) => p,
            _ => unreachable!("a page is started once"),
        }
    }
}

impl Guest for NativeGuest {
    fn tree(&mut self) -> Result<Vec<tree::Element>, String> {
        Ok(self.live().tree())
    }

    fn click(&mut self, id: usize) -> Result<bool, String> {
        Ok(self.live().click(id))
    }

    fn problems(&mut self) -> Vec<String> {
        self.live().problems()
    }

    fn set_url(&mut self, url: &cre::Url) -> Result<(), String> {
        match &mut self.0 {
            State::Waiting(u, _) => *u = url.clone(),
            State::Running(p) => p.set_url(url.clone()),
            State::Gone => {}
        }
        Ok(())
    }

    fn set_input(&mut self, id: usize, value: &str) -> Result<(), String> {
        self.live().set_input(id, value);
        Ok(())
    }

    fn net_take(&mut self) -> Result<Vec<cre::NetRequest>, String> {
        Ok(self.live().net_take())
    }

    fn net_done(&mut self, id: u32, result: cre::NetResult) -> Result<(), String> {
        self.live().net_done(id, result);
        Ok(())
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
