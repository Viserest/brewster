//! Running a page in this process: what tests, the browser's own pages and `NativeGuest`
//! use. A page module compiled to wasm runs the same code behind the exports in [`abi`].

use std::cell::RefCell;

use crate::{Element, Root, element::handler_of, element::take_problems};

pub struct LivePage {
    root: Root,
    /// Problems recorded while this page was built or ran a handler. They are collected
    /// right after each, so pages that share a thread keep their own.
    problems: RefCell<Vec<String>>,
}

impl LivePage {
    /// Builds the page by calling `build`; its element becomes the page.
    pub fn new(build: impl FnOnce() -> Element) -> LivePage {
        let root = Root::new();
        take_problems(); // not ours
        let e = build();
        root.tree.append(&e);
        LivePage {
            root,
            problems: RefCell::new(take_problems()),
        }
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
        (h.0)(&el, &self.root);
        self.problems.borrow_mut().extend(take_problems());
        true
    }

    /// The problems since the last call.
    pub fn problems(&self) -> Vec<String> {
        std::mem::take(&mut *self.problems.borrow_mut())
    }
}
