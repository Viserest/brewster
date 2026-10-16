//! Running a page in this process: what tests, the browser's own pages and `NativeGuest`
//! use. A page module compiled to wasm runs the same code behind the exports in [`abi`].

use std::cell::RefCell;

use crate::{
    Element, NetRequest, NetResult, Root, Url, element::handler_of, element::take_problems,
};

/// Marks a page function `fn() -> Element`.
pub struct NoRoot;
/// Marks a page function `fn(&Root) -> Element`.
pub struct WithRoot;

/// What `page!` and `router!` export: a function that builds the page, with or without the
/// root (so it can read `root.url()` while building).
pub trait PageFn<M> {
    fn build(&self, root: &Root) -> Element;
}

impl<F: Fn() -> Element> PageFn<NoRoot> for F {
    fn build(&self, _: &Root) -> Element {
        self()
    }
}

impl<F: Fn(&Root) -> Element> PageFn<WithRoot> for F {
    fn build(&self, root: &Root) -> Element {
        self(root)
    }
}

pub struct LivePage {
    root: Root,
    /// Problems recorded while this page was built or ran a handler. They are collected
    /// right after each, so pages that share a thread keep their own.
    problems: RefCell<Vec<String>>,
}

impl LivePage {
    /// Builds the page by calling `build`; its element becomes the page.
    pub fn new(build: impl FnOnce() -> Element) -> LivePage {
        LivePage::at(Url::default(), |_| build())
    }

    /// Builds the page at `url`: `build` is given the root, so it can read the address.
    pub fn at(url: Url, build: impl FnOnce(&Root) -> Element) -> LivePage {
        let root = Root::new();
        root.set_url(url);
        let entered = root.enter();
        take_problems(); // not ours
        let e = build(&root);
        root.tree.append(&e);
        drop(entered);
        // Something spawned while building may already be able to run.
        crate::task::run(&root);
        LivePage {
            root,
            problems: RefCell::new(take_problems()),
        }
    }

    /// The tab's address changed.
    pub fn set_url(&self, url: Url) {
        self.root.set_url(url);
    }

    /// The user edited the input with this id: the page sees the new value. `false` when
    /// there is no such input.
    pub fn set_input(&self, id: usize, value: &str) -> bool {
        match self.root.tree.find(id) {
            Some(e) if e.tag() == "input" => {
                e.set_text(value);
                true
            }
            _ => false,
        }
    }

    /// The network requests the page has queued since the last call.
    pub fn net_take(&self) -> Vec<NetRequest> {
        self.root.net().take()
    }

    /// The answer to a request. Futures waiting for it run before this returns.
    pub fn net_done(&self, id: u32, result: NetResult) {
        let _entered = self.root.enter();
        self.root.net().complete(id, result);
        crate::task::run(&self.root);
        self.problems.borrow_mut().extend(take_problems());
    }

    pub fn root(&self) -> &Root {
        &self.root
    }

    /// The page as plain elements, ready for layout.
    pub fn tree(&self) -> Vec<tree::Element> {
        self.root.tree.root.snapshot_children()
    }

    /// Whether the element with this id has a click handler.
    pub fn is_clickable(&self, id: usize) -> bool {
        handler_of(id).is_some() && self.root.tree.find(id).is_some()
    }

    /// Runs the element's handler. `false` when it has none.
    pub fn click(&self, id: usize) -> bool {
        let Some(el) = self.root.tree.find(id) else {
            return false;
        };
        // Cloned out first: the handler may build elements that register handlers too.
        let Some(h) = handler_of(id) else {
            return false;
        };
        take_problems();
        {
            let _entered = self.root.enter();
            (h.0)(&el, &self.root);
        }
        // A handler may have started something that can already make progress.
        crate::task::run(&self.root);
        self.problems.borrow_mut().extend(take_problems());
        true
    }

    /// The problems since the last call.
    pub fn problems(&self) -> Vec<String> {
        std::mem::take(&mut *self.problems.borrow_mut())
    }
}
