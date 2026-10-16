//! What a page compiled to wasm exports (see the `page!` macro). The browser host calls:
//!
//! * `cre_abi() -> u32`: the version of this interface ([`VERSION`]);
//! * `cre_alloc(len: u32) -> u32`: room for `len` bytes that the host fills before calling
//!   `cre_set_url`, `cre_input` or `cre_net_done` (one buffer, reused by every call);
//! * `cre_set_url(len: u32)`: the tab's address: a method byte, then the address as UTF-8.
//!   Called before the first `cre_page`, and again whenever the tab's address changes;
//! * `cre_page() -> u64`: `(ptr << 32) | len` of the page as a [`tree::wire`] tree;
//! * `cre_event(id: u32) -> u32`: runs the click handler of the element with that id,
//!   returns 1 when it had one;
//! * `cre_input(id: u32, len: u32)`: the user edited the input `id`; its new value (UTF-8)
//!   is in the buffer. Sent for every edited input before a click, so a handler sees what
//!   was typed;
//! * `cre_problems() -> u64`: like `cre_page`, but UTF-8 text, one problem per line;
//! * `cre_net_take() -> u64`: like `cre_page`, but the requests the page has queued
//!   ([`crate::encode_requests`]);
//! * `cre_net_done(id: u32, len: u32)`: the answer to request `id` is in the buffer
//!   ([`crate::encode_result`]). Futures waiting for it run before this returns.
//!
//! The memory behind a returned pointer stays valid until the next call to the same
//! export. The page owns its element tree and its closures; the host only ever sees bytes.
//! Pointers are 32 bits on wasm32; on other targets these exports are for tests only.

use std::cell::RefCell;

use crate::{LivePage, Method, PageFn, Url, net};

pub const VERSION: u32 = 2;

thread_local! {
    static PAGE: RefCell<Option<LivePage>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// Filled by the host through `cre_alloc`.
    static IN: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// The address the host gave, kept for a page that is not built yet.
    static URL: RefCell<Url> = RefCell::new(Url::default());
}

fn hand_out(bytes: Vec<u8>) -> u64 {
    OUT.with(|o| {
        *o.borrow_mut() = bytes;
        let o = o.borrow();
        ((o.as_ptr() as usize as u32 as u64) << 32) | o.len() as u64
    })
}

/// The first `len` bytes of what the host wrote.
fn input(len: u32) -> Vec<u8> {
    IN.with(|b| {
        let b = b.borrow();
        b[..(len as usize).min(b.len())].to_vec()
    })
}

pub fn alloc(len: u32) -> u32 {
    IN.with(|b| {
        let mut b = b.borrow_mut();
        b.clear();
        b.resize(len as usize, 0);
        b.as_mut_ptr() as usize as u32
    })
}

pub fn set_url(len: u32) {
    let bytes = input(len);
    let Some((&m, href)) = bytes.split_first() else {
        return;
    };
    let method = Method::from_code(m).unwrap_or_default();
    let url = Url::parse_with(method, &String::from_utf8_lossy(href));
    URL.with(|u| *u.borrow_mut() = url.clone());
    PAGE.with(|p| {
        if let Some(page) = p.borrow().as_ref() {
            page.set_url(url);
        }
    });
}

/// Builds the page on the first call (so it can read the address) and hands out its tree.
pub fn page<M, F: PageFn<M>>(build: F) -> u64 {
    let bytes = PAGE.with(|p| {
        let mut p = p.borrow_mut();
        let page = p.get_or_insert_with(|| {
            let url = URL.with(|u| u.borrow().clone());
            LivePage::at(url, |root| build.build(root))
        });
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

pub fn set_input(id: u32, len: u32) {
    let value = String::from_utf8_lossy(&input(len)).into_owned();
    PAGE.with(|p| {
        if let Some(pg) = p.borrow().as_ref() {
            pg.set_input(id as usize, &value);
        }
    });
}

pub fn problems() -> u64 {
    let text = PAGE
        .with(|p| p.borrow().as_ref().map(|pg| pg.problems()))
        .unwrap_or_default()
        .join("\n");
    hand_out(text.into_bytes())
}

pub fn net_take() -> u64 {
    let reqs = PAGE
        .with(|p| p.borrow().as_ref().map(|pg| pg.net_take()))
        .unwrap_or_default();
    hand_out(net::encode_requests(&reqs))
}

pub fn net_done(id: u32, len: u32) {
    let result = match net::decode_result(&input(len)) {
        Ok(r) => r,
        Err(e) => Err(net::NetworkError::Failed(format!("bad answer from the browser: {}", e))),
    };
    PAGE.with(|p| {
        if let Some(pg) = p.borrow().as_ref() {
            pg.net_done(id, result);
        }
    });
}

/// The page's tree as the host would decode it, read back from the last `page()` output.
#[cfg(test)]
#[allow(non_snake_case)]
pub fn PAGE_TREE_FOR_TESTS() -> Vec<tree::Element> {
    OUT.with(|o| tree::wire::decode(&o.borrow()).unwrap())
}
