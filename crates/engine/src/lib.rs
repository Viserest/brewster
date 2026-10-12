//! Engine: the whole pipeline (lex -> parse -> run scripts into a node tree -> layout)
//! behind one type. Apps and the browser depend on this crate only.
//!
//! `render_source` is the one-shot path. Interactive code loads a [`Document`] once and
//! lays it out again whenever the width, the form state or the tree changes.

use std::cell::RefCell;
use std::rc::Rc;

pub use cursor::{
    Cursor, Mode,
    edit::{self, EditKey},
    scroll_into_view,
};
pub use layout::{
    Form, InputLayout, Kind, LayoutOptions, LineInfo, Page, Rgb, Row, Span, row_text,
};
pub use location::{DefaultLoader, Loader, Url};
pub use plugin::Registry;

/// A loaded page: its node tree (built by running the document and the plugins installed
/// for its application) and the scripts that can change it.
pub struct Document {
    page: script::Page,
}

impl Document {
    /// Loads `src` as an anonymous page with no plugins (`about:blank`).
    pub fn parse(src: &str) -> Result<Document, String> {
        Document::load(
            src,
            Url::about("blank"),
            Rc::new(RefCell::new(Registry::in_memory())),
            Rc::new(DefaultLoader::default()),
        )
    }

    /// Loads `src` as the page at `url`. The page runs first, then the plugins of its
    /// application in insertion order (see `plugin`).
    pub fn load(
        src: &str,
        url: Url,
        registry: Rc<RefCell<Registry>>,
        loader: Rc<dyn Loader>,
    ) -> Result<Document, String> {
        Ok(Document {
            page: script::Page::load(src, url, registry, loader)?,
        })
    }

    /// The text of the first heading (`h1`, else `h2` ...), used as the page's title.
    pub fn title(&self) -> Option<String> {
        for tag in ["h1", "h2", "h3", "h4", "h5", "h6"] {
            if let Ok(n) = self.page.root().query(tag) {
                let t = n.text();
                let t = t.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
        None
    }

    pub fn url(&self) -> &Url {
        &self.page.url
    }

    /// Plugin problems and `log(...)` output do not stop a page; they are collected here.
    pub fn diagnostics(&self) -> &[String] {
        &self.page.diagnostics
    }

    pub fn take_log(&mut self) -> Vec<String> {
        self.page.take_log()
    }

    /// Lays the page out in `width` cells with the given input values and caret.
    pub fn layout(&self, width: usize, form: &Form, opts: LayoutOptions) -> Result<Page, String> {
        layout::layout_all_with(&self.page.elements(), width, form, opts)
    }

    /// Runs the `onclick` function of the element with tree id `node`. Returns whether it
    /// had one; errors are the script's error message.
    pub fn click(&mut self, node: usize) -> Result<bool, String> {
        let Some(n) = self.page.root().find(node) else {
            return Ok(false);
        };
        if !script::Page::is_clickable(&n) {
            return Ok(false);
        }
        self.page.click(&n)?;
        Ok(true)
    }

    /// Whether the element with tree id `node` has an `onclick` function.
    pub fn is_clickable(&self, node: usize) -> bool {
        self.page
            .root()
            .find(node)
            .is_some_and(|n| script::Page::is_clickable(&n))
    }
}

/// Renders source text into rows that are `width` cells wide (no scripts' side effects
/// survive; plugins are not run).
pub fn render_source(src: &str, width: usize) -> Result<Vec<Row>, String> {
    Ok(Document::parse(src)?
        .layout(width, &Form::default(), LayoutOptions::default())?
        .rows)
}

#[cfg(test)]
mod tests;
