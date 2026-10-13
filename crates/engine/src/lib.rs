//! Engine: the whole pipeline (fetch -> compile to wasm if needed -> run the page and its
//! plugins -> element tree -> layout) behind one type. Apps and the browser depend on this
//! crate only.
//!
//! `render_url` is the one-shot path. Interactive code loads a [`Document`] once and
//! lays it out again whenever the width, the form state or the tree changes.

use std::cell::RefCell;
use std::rc::Rc;

pub use cursor::{
    Cursor, Mode,
    edit::{self, EditKey},
    scroll_into_view,
};
pub use host::{Guest, Host, MapRunner, Runner};
pub use layout::{
    Form, InputLayout, Kind, LayoutOptions, LineInfo, Page, Rgb, Row, Span, row_text,
};
pub use location::{DefaultLoader, Loader, Url};
pub use plugin::Registry;

use host::{NativeGuest, Session};

/// A loaded page: the elements its module built (and its plugins' elements after them),
/// plus the means to run their click handlers.
pub struct Document {
    url: Url,
    session: Session,
}

impl Document {
    /// A page built in this process: the browser's own pages and tests. It runs no
    /// plugins.
    pub fn from_page(url: Url, build: impl FnOnce() -> cre::Element) -> Document {
        Document::from_guest(url, Box::new(NativeGuest::new(build)))
            .expect("a page that runs in this process cannot fail")
    }

    /// A page run by any [`Guest`], without plugins.
    pub fn from_guest(url: Url, guest: Box<dyn Guest>) -> Result<Document, String> {
        let mut session = Session::new();
        session.set_page(guest);
        session.refresh()?;
        Ok(Document { url, session })
    }

    /// `about:blank`.
    pub fn blank() -> Document {
        Document::from_page(Url::about("blank"), || cre::Element::new(cre::Tag::Box))
    }

    /// Fetches the page at `url` (wasm, or Rust source that is compiled; see `plugin`) and
    /// runs it, then the plugins of its application in insertion order.
    pub fn load(
        url: Url,
        registry: Rc<RefCell<Registry>>,
        loader: Rc<dyn Loader>,
        host: Rc<Host>,
    ) -> Result<Document, String> {
        let bytes = loader
            .load_bytes(&url)
            .map_err(|e| format!("{}: {}", url, e))?;
        Document::from_bytes(bytes, url, registry, loader, host)
    }

    /// Like [`Document::load`] with the page already in hand.
    pub fn from_bytes(
        bytes: Vec<u8>,
        url: Url,
        registry: Rc<RefCell<Registry>>,
        loader: Rc<dyn Loader>,
        host: Rc<Host>,
    ) -> Result<Document, String> {
        let module = plugin::module_from_bytes(bytes, &url, &*loader, &host.compiler)
            .map_err(|e| e.to_string())?;
        let mut session = Session::new();
        session.set_page(host.runner.instantiate(&module)?);

        // The plugins to run are fixed before the page runs. The registry is not borrowed
        // while anything runs.
        let modules = registry
            .borrow_mut()
            .modules_for(&url, &*loader, &host.compiler);
        for (id, module) in modules {
            let started = module
                .map_err(|e| e.to_string())
                .and_then(|m| host.runner.instantiate(&m));
            match started {
                Ok(g) => session.add_plugin(&id.to_string(), g),
                Err(e) => session.diagnostics.push(format!("plugin {}: {}", id, e)),
            }
        }
        session.refresh()?;
        Ok(Document { url, session })
    }

    /// The text of the first heading (`h1`, else `h2` ...), used as the page's title.
    pub fn title(&self) -> Option<String> {
        fn first(es: &[tree::Element], tag: &str) -> Option<String> {
            for e in es {
                if e.tag == tag {
                    let t = e.args.first().map(|s| s.trim()).unwrap_or("");
                    if !t.is_empty() {
                        return Some(t.to_string());
                    }
                }
                if let Some(t) = first(&e.children, tag) {
                    return Some(t);
                }
            }
            None
        }
        ["h1", "h2", "h3", "h4", "h5", "h6"]
            .iter()
            .find_map(|tag| first(self.session.tree(), tag))
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Plugin problems do not stop a page; they are collected here.
    pub fn diagnostics(&self) -> &[String] {
        &self.session.diagnostics
    }

    /// Lays the page out in `width` cells with the given input values and caret.
    pub fn layout(&self, width: usize, form: &Form, opts: LayoutOptions) -> Result<Page, String> {
        layout::layout_all_with(self.session.tree(), width, form, opts)
    }

    /// Runs the `onclick` handler of the element with tree id `node`. Returns whether it
    /// had one. The page is read again afterwards, so call `layout` to see the change.
    pub fn click(&mut self, node: usize) -> Result<bool, String> {
        self.session.click(node)
    }

    /// Whether the element with tree id `node` has an `onclick` handler.
    pub fn is_clickable(&self, node: usize) -> bool {
        self.session.is_clickable(node)
    }
}

/// Loads the page at `url` and renders it into rows that are `width` cells wide (plugins
/// are not run).
pub fn render_url(
    url: &Url,
    width: usize,
    loader: Rc<dyn Loader>,
    host: Rc<Host>,
) -> Result<Vec<Row>, String> {
    let registry = Rc::new(RefCell::new(Registry::in_memory()));
    Ok(Document::load(url.clone(), registry, loader, host)?
        .layout(width, &Form::default(), LayoutOptions::default())?
        .rows)
}

/// Renders a page built in this process (see [`Document::from_page`]).
pub fn render_page(
    build: impl FnOnce() -> cre::Element,
    width: usize,
) -> Result<Vec<Row>, String> {
    Ok(Document::from_page(Url::about("blank"), build)
        .layout(width, &Form::default(), LayoutOptions::default())?
        .rows)
}

#[cfg(test)]
mod tests;
