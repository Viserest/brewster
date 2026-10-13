//! [`Element`]: a handle to a node of the page. Cloning shares the node, so a handler
//! can hold one while the page keeps it (`set_text` through a clone changes the page).

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use tree::Node;

use crate::{Attr, Handler, InputType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Box,
    Link,
    P,
    Span,
    Input,
    Button,
    Pre,
    /// `H(1)` .. `H(6)`.
    H(u8),
}

impl Tag {
    pub fn name(self) -> &'static str {
        match self {
            Tag::Box => "box",
            Tag::Link => "link",
            Tag::P => "p",
            Tag::Span => "span",
            Tag::Input => "input",
            Tag::Button => "button",
            Tag::Pre => "pre",
            Tag::H(1) => "h1",
            Tag::H(2) => "h2",
            Tag::H(3) => "h3",
            Tag::H(4) => "h4",
            Tag::H(5) => "h5",
            Tag::H(_) => "h6",
        }
    }
}

thread_local! {
    /// Click handlers by node id. Ids are unique in a process, so pages that share a
    /// thread (tests) never meet each other's handlers.
    static HANDLERS: RefCell<HashMap<usize, Handler>> = RefCell::new(HashMap::new());
    /// Mistakes that are not compile errors (children on a leaf, say).
    static PROBLEMS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn handler_of(id: usize) -> Option<Handler> {
    HANDLERS.with(|h| h.borrow().get(&id).cloned())
}

/// Takes the problems recorded since the last call.
pub fn take_problems() -> Vec<String> {
    PROBLEMS.with(|p| std::mem::take(&mut *p.borrow_mut()))
}

fn problem(msg: String) {
    PROBLEMS.with(|p| p.borrow_mut().push(msg));
}

#[derive(Default)]
struct Meta {
    before: Vec<Element>,
    after: Vec<Element>,
}

#[derive(Clone)]
pub struct Element {
    node: Node,
    meta: Rc<RefCell<Meta>>,
}

impl std::fmt::Debug for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.node)
    }
}

impl Element {
    pub fn new(tag: Tag) -> Element {
        let node = Node::create(tag.name()).expect("every Tag is a known tag");
        Element::from_node(node)
    }

    pub(crate) fn from_node(node: Node) -> Element {
        Element {
            node,
            meta: Rc::new(RefCell::new(Meta::default())),
        }
    }

    pub fn node(&self) -> &Node {
        &self.node
    }

    pub fn id(&self) -> usize {
        self.node.id()
    }

    pub fn tag(&self) -> String {
        self.node.tag()
    }

    // ── attributes ──────────────────────────────────────────────────

    pub fn set(&self, attr: Attr) -> &Element {
        match attr {
            Attr::extends(t) => self.extend_from(&t),
            Attr::before(e) => self.meta.borrow_mut().before.push(e),
            Attr::after(e) => self.meta.borrow_mut().after.push(e),
            Attr::onclick(h) => {
                HANDLERS.with(|m| m.borrow_mut().insert(self.id(), h));
                self.node.set_style("onclick", vec!["1".to_string()]);
            }
            plain => {
                let values = plain.values().expect("plain attributes have values");
                self.node.set_style(plain.key(), values);
            }
        }
        self
    }

    /// The style values stored under `key` (`"border-color"`, `"pad"` ...).
    pub fn style(&self, key: &str) -> Option<Vec<String>> {
        self.node.style_get(key)
    }

    fn extend_from(&self, t: &Element) {
        for (k, v) in t.node.style() {
            // Own attributes win; behaviour and identity are not inherited.
            if k == "onclick" || k == "id" || self.node.style_get(&k).is_some() {
                continue;
            }
            self.node.set_style(&k, v);
        }
        let (before, after) = {
            let m = t.meta.borrow();
            (m.before.clone(), m.after.clone())
        };
        let mut me = self.meta.borrow_mut();
        // The template's content comes first, so what this box adds can follow it.
        let mut b: Vec<Element> = before.iter().map(Element::deep_clone).collect();
        b.append(&mut me.before);
        me.before = b;
        me.after.extend(after.iter().map(Element::deep_clone));
    }

    /// Input type: `Password` hides the value.
    pub fn set_input_type(&self, t: InputType) {
        match t {
            InputType::Password => self.node.set_style("protected", vec![]),
            InputType::Text => {}
        }
    }

    // ── content ─────────────────────────────────────────────────────

    /// Sets the visible text: a link's label, an input's value, or the text of any other
    /// leaf.
    pub fn set_text(&self, text: impl Into<String>) {
        let i = match self.node.tag().as_str() {
            "link" | "input" => 1,
            _ => 0,
        };
        self.node.set_arg(i, &text.into());
    }

    pub fn text(&self) -> String {
        self.node.text()
    }

    pub fn args(&self) -> Vec<String> {
        self.node.args()
    }

    pub fn set_args(&self, args: Vec<String>) {
        self.node.set_args(args);
    }

    // ── tree ────────────────────────────────────────────────────────

    /// Appends `child`. Only boxes take children; anything else records a problem.
    pub fn push(&self, child: Element) -> &Element {
        if let Err(e) = self.node.append(&child.node) {
            problem(format!("<{}>: {}", self.tag(), e));
        }
        self
    }

    pub fn extend(&self, children: impl IntoIterator<Item = Element>) -> &Element {
        for c in children {
            self.push(c);
        }
        self
    }

    pub fn children(&self) -> Vec<Element> {
        self.node.children().into_iter().map(Element::from_node).collect()
    }

    pub fn parent(&self) -> Option<Element> {
        self.node.parent().map(Element::from_node)
    }

    pub fn remove(&self) {
        self.node.remove();
    }

    /// The first descendant matching `selector` (`h1`, `#id`, `tag#id`, `*`).
    pub fn query(&self, selector: &str) -> Option<Element> {
        self.node.query(selector).ok().map(Element::from_node)
    }

    pub fn query_all(&self, selector: &str) -> Vec<Element> {
        self.node
            .query_all(selector)
            .unwrap_or_default()
            .into_iter()
            .map(Element::from_node)
            .collect()
    }

    /// A detached copy of this element and everything under it, handlers included.
    pub fn deep_clone(&self) -> Element {
        let copy = self.node.deep_clone();
        copy_handlers(&self.node, &copy);
        Element::from_node(copy)
    }

    /// Puts `before` and `after` content around the children. The tag macros call this
    /// once the children are in.
    #[doc(hidden)]
    pub fn finish(self) -> Element {
        let (before, after) = {
            let m = self.meta.borrow();
            (m.before.clone(), m.after.clone())
        };
        for e in before.iter().rev() {
            if let Err(err) = self.node.prepend(&e.node) {
                problem(format!("<{}>: before: {}", self.tag(), err));
            }
        }
        for e in after {
            if let Err(err) = self.node.append(&e.node) {
                problem(format!("<{}>: after: {}", self.tag(), err));
            }
        }
        self
    }
}

fn copy_handlers(from: &Node, to: &Node) {
    if let Some(h) = handler_of(from.id()) {
        HANDLERS.with(|m| m.borrow_mut().insert(to.id(), h));
    }
    for (a, b) in from.children().iter().zip(to.children().iter()) {
        copy_handlers(a, b);
    }
}
