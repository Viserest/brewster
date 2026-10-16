//! [`Root`]: what a page and its handlers use to reach the rest of the world: the element
//! tree, the page's address, the network and the executor. Handlers are given a `&Root`;
//! `root()` returns the same thing for code that has none at hand.

use std::{cell::RefCell, future::Future, rc::Rc};

use tree::Node;

use crate::{Element, Net, Url, task::Tasks};

pub(crate) struct Inner {
    url: RefCell<Url>,
    net: Net,
    pub(crate) tasks: RefCell<Tasks>,
}

/// A page's handle to its world. Cloning shares it.
#[derive(Clone)]
pub struct Root {
    pub tree: Tree,
    pub(crate) inner: Rc<Inner>,
}

/// The page's element tree.
#[derive(Clone)]
pub struct Tree {
    pub(crate) root: Node,
}

thread_local! {
    /// The roots of the pages that are running on this thread, innermost last. A page
    /// module is its own program, so it only ever has one; native pages (tests, the
    /// browser's own) can nest.
    static CURRENT: RefCell<Vec<Root>> = const { RefCell::new(Vec::new()) };
}

/// The root of the page that is running right now: during its build, in a click handler
/// and in a spawned task.
///
/// # Panics
/// Outside of a running page.
pub fn root() -> Root {
    CURRENT.with(|c| {
        c.borrow()
            .last()
            .cloned()
            .expect("cre::root() was called outside of a running page")
    })
}

/// Runs `fut` to completion in the background (see [`Root::spawn`]).
pub fn spawn(fut: impl Future<Output = ()> + 'static) {
    root().spawn(fut)
}

pub(crate) struct Entered;

impl Drop for Entered {
    fn drop(&mut self) {
        CURRENT.with(|c| {
            c.borrow_mut().pop();
        });
    }
}

impl Root {
    pub(crate) fn new() -> Root {
        Root {
            tree: Tree {
                root: Node::new_root(),
            },
            inner: Rc::new(Inner {
                url: RefCell::new(Url::default()),
                net: Net::new(),
                tasks: RefCell::new(Tasks::new()),
            }),
        }
    }

    /// Makes this the page `root()` answers for until the guard is dropped.
    pub(crate) fn enter(&self) -> Entered {
        CURRENT.with(|c| c.borrow_mut().push(self.clone()));
        Entered
    }

    /// Where the page is: the address of the browser tab it runs in. It changes when the
    /// tab's address does.
    pub fn url(&self) -> Url {
        self.inner.url.borrow().clone()
    }

    pub(crate) fn set_url(&self, url: Url) {
        *self.inner.url.borrow_mut() = url;
    }

    /// The network: `root.net().post("/login", body).await`.
    pub fn net(&self) -> Net {
        self.inner.net.clone()
    }

    /// Runs a future in the background. It is polled now and again whenever something it
    /// waits for happens (a network answer). The page is drawn again after each.
    pub fn spawn(&self, fut: impl Future<Output = ()> + 'static) {
        self.inner.tasks.borrow_mut().add(Box::pin(fut));
        crate::task::run(self);
    }

    /// How many spawned futures have not finished.
    pub fn pending_tasks(&self) -> usize {
        self.inner.tasks.borrow().pending()
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
