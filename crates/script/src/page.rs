//! A loaded page: its document tree, its scripts and the plugins that ran with it.

use crate::interp::{Interpreter, Services};

use std::{cell::RefCell, rc::Rc};

use location::{Loader, Url};
use plugin::Registry;
use tree::{Element, Node};

pub struct Page {
    pub url: Url,
    pub interp: Interpreter,
    /// Problems that did not stop the page: plugins that failed to load or run.
    pub diagnostics: Vec<String>,
}

impl Page {
    /// Runs the document, then every plugin installed for its application, in the order
    /// they were installed. A broken document is an error; a broken plugin is a diagnostic.
    pub fn load(
        src: &str,
        url: Url,
        registry: Rc<RefCell<Registry>>,
        loader: Rc<dyn Loader>,
    ) -> Result<Page, String> {
        let program = parser::parse_source(src)?;
        let root = Node::new_root();
        let svc = Services {
            url: url.clone(),
            registry: registry.clone(),
            loader: loader.clone(),
        };
        let mut interp = Interpreter::new(root, svc);

        // The plugins to run are fixed before the document runs, so a plugin installed by
        // the page waits for the next load. The registry is not borrowed while anything
        // runs: scripts may install plugins.
        let programs = registry.borrow_mut().programs_for(&url, &*loader);
        interp.run(&program)?;
        let mut diagnostics = Vec::new();
        for (id, prog) in programs {
            match prog {
                Ok(stmts) => {
                    if let Err(e) = interp.run(&stmts) {
                        diagnostics.push(format!("plugin {}: {}", id, e));
                    }
                }
                Err(e) => diagnostics.push(format!("plugin {}: {}", id, e)),
            }
        }
        Ok(Page {
            url,
            interp,
            diagnostics,
        })
    }

    pub fn root(&self) -> &Node {
        self.interp.root()
    }

    /// The elements layout draws.
    pub fn elements(&self) -> Vec<Element> {
        self.interp.root().snapshot_children()
    }

    /// Runs a node's `onclick` function, if it has one.
    pub fn click(&mut self, node: &Node) -> Result<(), String> {
        self.interp.click(node)
    }

    /// Whether `node` has an `onclick` handler.
    pub fn is_clickable(node: &Node) -> bool {
        node.style_get("onclick").is_some()
    }

    pub fn take_log(&mut self) -> Vec<String> {
        self.interp.take_log()
    }
}
