//! What a page compiled to wasm exports (see the `page!` macro). The browser host calls:
//!
//! * `cre_abi() -> u32`: the version of this interface (1);
//! * `cre_page() -> u64`: `(ptr << 32) | len` of the page as a [`tree::wire`] tree;
//! * `cre_event(id: u32) -> u32`: runs the click handler of the element with that id,
//!   returns 1 when it had one;
//! * `cre_problems() -> u64`: like `cre_page`, but UTF-8 text, one problem per line.
//!
//! The memory behind a returned pointer stays valid until the next call to the same
//! export. The page owns its element tree and its closures; the host only ever sees bytes.
//! Pointers are 32 bits on wasm32; on other targets these exports are for tests only.

use std::cell::RefCell;

use crate::{Element, LivePage};

pub const VERSION: u32 = 1;

thread_local! {
    static PAGE: RefCell<Option<LivePage>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn hand_out(bytes: Vec<u8>) -> u64 {
    OUT.with(|o| {
        *o.borrow_mut() = bytes;
        let o = o.borrow();
        ((o.as_ptr() as usize as u32 as u64) << 32) | o.len() as u64
    })
}

pub fn page(build: fn() -> Element) -> u64 {
    let bytes = PAGE.with(|p| {
        let mut p = p.borrow_mut();
        let page = p.get_or_insert_with(|| LivePage::new(build));
        tree::wire::encode(&page.tree())
    });
    hand_out(bytes)
}

pub fn event(id: u32) -> u32 {
    // Handlers never call the exports, so a shared borrow across the call is fine.
    PAGE.with(|p| match p.borrow().as_ref() {
        Some(pg) => pg.click(id as usize) as u32,
        None => 0,
    })
}

pub fn problems() -> u64 {
    let text = PAGE
        .with(|p| p.borrow().as_ref().map(|pg| pg.problems()))
        .unwrap_or_default()
        .join("\n");
    hand_out(text.into_bytes())
}

/// The page's tree as the host would decode it, read back from the last `page()` output.
#[cfg(test)]
#[allow(non_snake_case)]
pub fn PAGE_TREE_FOR_TESTS() -> Vec<tree::Element> {
    OUT.with(|o| tree::wire::decode(&o.borrow()).unwrap())
}
