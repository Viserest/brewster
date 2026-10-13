//! [`Root`]: what a handler is given to reach the page. It is passed in, never global.

use tree::Node;

use crate::Element;

pub struct Root {
    pub tree: Tree,
}

/// The page's element tree.
#[derive(Clone)]
pub struct Tree {
    pub(crate) root: Node,
}

impl Root {
    pub(crate) fn new() -> Root {
        Root {
            tree: Tree {
                root: Node::new_root(),
            },
        }
    }
}

impl Tree {
    /// The first element matching `selector` (`h1`, `#id`, `tag#id`, `*`).
    pub fn query(&self, selector: &str) -> Option<Element> {
        self.root.query(selector).ok().map(Element::from_node)
    }

    pub fn query_all(&self, selector: &str) -> Vec<Element> {
        self.root
            .query_all(selector)
            .unwrap_or_default()
            .into_iter()
            .map(Element::from_node)
            .collect()
    }

    pub fn find(&self, id: usize) -> Option<Element> {
        self.root.find(id).map(Element::from_node)
    }

    /// Adds `e` at the end of the page.
    pub fn append(&self, e: &Element) {
        let _ = self.root.append(e.node());
    }
}
