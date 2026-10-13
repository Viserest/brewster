//! The document tree.
//!
//! [`Node`] is the live, mutable tree that scripts edit through `root.tree`; it is shared
//! (`Rc`) so a script can hold a node while the page keeps it. [`Element`] is a plain
//! snapshot of a node and its subtree: the input of layout.

pub mod wire;

use std::{
    cell::RefCell,
    collections::BTreeMap,
    fmt,
    rc::{Rc, Weak},
    sync::atomic::{AtomicUsize, Ordering},
};

pub type Style = BTreeMap<String, Vec<String>>;

/// Every element a page can contain.
pub const TAGS: &[&str] = &[
    "box", "link", "p", "span", "input", "button", "pre", "h1", "h2", "h3", "h4", "h5", "h6",
];

pub fn is_tag(s: &str) -> bool {
    TAGS.contains(&s)
}

/// A snapshot of one element and its children. `id` is the id of the node it came from.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Element {
    pub tag: String,
    pub style: Style,
    pub args: Vec<String>,
    pub children: Vec<Element>,
    pub id: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    Empty,
    Invalid(String),
    NotFound(String),
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueryError::Empty => write!(f, "empty selector"),
            QueryError::Invalid(s) => write!(f, "invalid selector: {}", s),
            QueryError::NotFound(s) => write!(f, "nothing matches `{}`", s),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeError {
    UnknownTag(String),
    NotAContainer(String),
    Cycle,
}

impl fmt::Display for NodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeError::UnknownTag(t) => write!(f, "unknown tag `{}`", t),
            NodeError::NotAContainer(t) => write!(f, "`{}` cannot have children", t),
            NodeError::Cycle => write!(f, "a node cannot be appended to itself or its descendant"),
        }
    }
}

static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

struct Data {
    id: usize,
    tag: String,
    style: Style,
    args: Vec<String>,
    children: Vec<Node>,
    parent: Weak<RefCell<Data>>,
}

/// A live node. Cloning shares the node.
#[derive(Clone)]
pub struct Node(Rc<RefCell<Data>>);

impl PartialEq for Node {
    fn eq(&self, other: &Node) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d = self.0.borrow();
        write!(f, "Node#{}<{}>", d.id, d.tag)
    }
}

fn new_data(tag: &str) -> Node {
    Node(Rc::new(RefCell::new(Data {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        tag: tag.to_string(),
        style: Style::new(),
        args: Vec::new(),
        children: Vec::new(),
        parent: Weak::new(),
    })))
}

impl Node {
    /// The root of a document. It is not drawn; its children are the page.
    pub fn new_root() -> Node {
        new_data("root")
    }

    /// A detached node of a known tag.
    pub fn create(tag: &str) -> Result<Node, NodeError> {
        if !is_tag(tag) {
            return Err(NodeError::UnknownTag(tag.to_string()));
        }
        Ok(new_data(tag))
    }

    pub fn id(&self) -> usize {
        self.0.borrow().id
    }

    pub fn tag(&self) -> String {
        self.0.borrow().tag.clone()
    }

    pub fn style(&self) -> Style {
        self.0.borrow().style.clone()
    }

    pub fn style_get(&self, key: &str) -> Option<Vec<String>> {
        self.0.borrow().style.get(key).cloned()
    }

    pub fn set_style(&self, key: &str, values: Vec<String>) {
        self.0.borrow_mut().style.insert(key.to_string(), values);
    }

    pub fn set_style_map(&self, style: Style) {
        self.0.borrow_mut().style = style;
    }

    pub fn args(&self) -> Vec<String> {
        self.0.borrow().args.clone()
    }

    pub fn set_args(&self, args: Vec<String>) {
        self.0.borrow_mut().args = args;
    }

    /// Sets argument `i`, padding with empty strings when it does not exist yet.
    pub fn set_arg(&self, i: usize, value: &str) {
        let mut d = self.0.borrow_mut();
        while d.args.len() <= i {
            d.args.push(String::new());
        }
        d.args[i] = value.to_string();
    }

    pub fn children(&self) -> Vec<Node> {
        self.0.borrow().children.clone()
    }

    pub fn parent(&self) -> Option<Node> {
        self.0.borrow().parent.upgrade().map(Node)
    }

    fn is_container(&self) -> bool {
        let t = self.0.borrow();
        t.tag == "root" || t.tag == "box"
    }

    fn has_ancestor_or_is(&self, other: &Node) -> bool {
        let mut cur = Some(self.clone());
        while let Some(n) = cur {
            if &n == other {
                return true;
            }
            cur = n.parent();
        }
        false
    }

    /// Appends `child` as the last child (moving it when it already has a parent) and
    /// returns it.
    pub fn append(&self, child: &Node) -> Result<Node, NodeError> {
        if !self.is_container() {
            return Err(NodeError::NotAContainer(self.tag()));
        }
        if self.has_ancestor_or_is(child) {
            return Err(NodeError::Cycle);
        }
        child.remove();
        child.0.borrow_mut().parent = Rc::downgrade(&self.0);
        self.0.borrow_mut().children.push(child.clone());
        Ok(child.clone())
    }

    /// Detaches this node from its parent.
    pub fn remove(&self) {
        if let Some(p) = self.parent() {
            p.0.borrow_mut().children.retain(|c| c != self);
        }
        self.0.borrow_mut().parent = Weak::new();
    }

    /// The first descendant (in document order) that matches `selector`.
    pub fn query(&self, selector: &str) -> Result<Node, QueryError> {
        self.query_all(selector)?
            .into_iter()
            .next()
            .ok_or_else(|| QueryError::NotFound(selector.trim().to_string()))
    }

    /// Every descendant that matches `selector`, in document order.
    ///
    /// Selectors: `*`, a tag (`h1`), `#id` (the element's `id` style), or `tag#id`.
    pub fn query_all(&self, selector: &str) -> Result<Vec<Node>, QueryError> {
        let sel = Selector::parse(selector)?;
        let mut out = Vec::new();
        self.collect(&sel, &mut out);
        Ok(out)
    }

    fn collect(&self, sel: &Selector, out: &mut Vec<Node>) {
        for c in self.children() {
            if sel.matches(&c) {
                out.push(c.clone());
            }
            c.collect(sel, out);
        }
    }

    /// This node or a descendant with the given id.
    pub fn find(&self, id: usize) -> Option<Node> {
        if self.id() == id {
            return Some(self.clone());
        }
        self.children().iter().find_map(|c| c.find(id))
    }

    /// The text of this node: its own text arguments, else its descendants' text, one line
    /// each.
    pub fn text(&self) -> String {
        let d = self.0.borrow();
        let own: Vec<&str> = match d.tag.as_str() {
            "link" => d
                .args
                .get(1)
                .or(d.args.first())
                .map(|s| s.as_str())
                .into_iter()
                .collect(),
            "input" => d.args.get(1).map(|s| s.as_str()).into_iter().collect(),
            "box" | "root" => Vec::new(),
            _ => d.args.first().map(|s| s.as_str()).into_iter().collect(),
        };
        if !own.is_empty() || (d.tag != "box" && d.tag != "root") {
            return own.join("");
        }
        let kids = d.children.clone();
        drop(d);
        kids.iter()
            .map(|c| c.text())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A detached copy of this node and its subtree, with new ids.
    pub fn deep_clone(&self) -> Node {
        let d = self.0.borrow();
        let copy = new_data(&d.tag);
        {
            let mut c = copy.0.borrow_mut();
            c.style = d.style.clone();
            c.args = d.args.clone();
        }
        for k in &d.children {
            let kc = k.deep_clone();
            kc.0.borrow_mut().parent = Rc::downgrade(&copy.0);
            copy.0.borrow_mut().children.push(kc);
        }
        drop(d);
        copy
    }

    /// Adds `child` as the first child (moving it when it already has a parent).
    pub fn prepend(&self, child: &Node) -> Result<Node, NodeError> {
        self.append(child)?;
        let mut d = self.0.borrow_mut();
        let last = d.children.pop().expect("just appended");
        d.children.insert(0, last);
        Ok(child.clone())
    }

    /// A snapshot of this node and its subtree.
    pub fn snapshot(&self) -> Element {
        let d = self.0.borrow();
        Element {
            tag: d.tag.clone(),
            style: d.style.clone(),
            args: d.args.clone(),
            children: d.children.iter().map(|c| c.snapshot()).collect(),
            id: d.id,
        }
    }

    /// Snapshots of this node's children: the elements layout draws for a root.
    pub fn snapshot_children(&self) -> Vec<Element> {
        self.children().iter().map(|c| c.snapshot()).collect()
    }
}

struct Selector {
    tag: Option<String>,
    id: Option<String>,
}

impl Selector {
    fn parse(s: &str) -> Result<Selector, QueryError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(QueryError::Empty);
        }
        let (tag, id) = match s.split_once('#') {
            Some((t, i)) => (t, Some(i)),
            None => (s, None),
        };
        if let Some(i) = id {
            let ok = !i.is_empty()
                && i.chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '-');
            if !ok {
                return Err(QueryError::Invalid(s.to_string()));
            }
        }
        let tag = match tag {
            "" | "*" => None,
            t if is_tag(t) => Some(t.to_string()),
            _ => return Err(QueryError::Invalid(format!("unknown tag in `{}`", s))),
        };
        if tag.is_none() && id.is_none() && s != "*" {
            return Err(QueryError::Invalid(s.to_string()));
        }
        Ok(Selector {
            tag,
            id: id.map(|i| i.to_string()),
        })
    }

    fn matches(&self, n: &Node) -> bool {
        if let Some(t) = &self.tag {
            if &n.tag() != t {
                return false;
            }
        }
        if let Some(id) = &self.id {
            if n.style_get("id")
                .and_then(|v| v.into_iter().next())
                .as_deref()
                != Some(id.as_str())
            {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (Node, Node, Node, Node) {
        let root = Node::new_root();
        let b = Node::create("box").unwrap();
        let h = Node::create("h1").unwrap();
        let p = Node::create("p").unwrap();
        h.set_arg(0, "Title");
        p.set_arg(0, "Body");
        p.set_style("id", vec!["main".to_string()]);
        root.append(&b).unwrap();
        b.append(&h).unwrap();
        b.append(&p).unwrap();
        (root, b, h, p)
    }

    #[test]
    fn find_by_id_searches_the_subtree() {
        let (root, a, _b, c) = tree();
        assert_eq!(root.find(c.id()), Some(c.clone()));
        assert_eq!(root.find(a.id()), Some(a.clone()));
        assert_eq!(a.find(root.id()), None);
        assert_eq!(root.find(usize::MAX), None);
    }

    #[test]
    fn create_rejects_unknown_tags() {
        assert_eq!(
            Node::create("blink").unwrap_err(),
            NodeError::UnknownTag("blink".to_string())
        );
        assert!(Node::create("root").is_err());
    }

    #[test]
    fn append_builds_the_tree_and_parents() {
        let (root, b, h, p) = tree();
        assert_eq!(root.children(), vec![b.clone()]);
        assert_eq!(b.children(), vec![h.clone(), p.clone()]);
        assert_eq!(h.parent(), Some(b.clone()));
        assert_eq!(root.parent(), None);
    }

    #[test]
    fn append_moves_an_attached_node() {
        let (root, b, h, _) = tree();
        let other = Node::create("box").unwrap();
        root.append(&other).unwrap();
        other.append(&h).unwrap();
        assert_eq!(h.parent(), Some(other.clone()));
        assert!(!b.children().contains(&h));
    }

    #[test]
    fn append_refuses_cycles_and_leaves() {
        let (root, b, h, _) = tree();
        assert_eq!(b.append(&b), Err(NodeError::Cycle));
        assert_eq!(b.append(&root), Err(NodeError::Cycle));
        let p2 = Node::create("p").unwrap();
        assert_eq!(
            h.append(&p2),
            Err(NodeError::NotAContainer("h1".to_string()))
        );
    }

    #[test]
    fn remove_detaches() {
        let (_, b, h, _) = tree();
        h.remove();
        assert_eq!(h.parent(), None);
        assert_eq!(b.children().len(), 1);
    }

    #[test]
    fn query_by_tag_id_and_wildcard() {
        let (root, _, h, p) = tree();
        assert_eq!(root.query("h1").unwrap(), h);
        assert_eq!(root.query("#main").unwrap(), p);
        assert_eq!(root.query("p#main").unwrap(), p);
        assert_eq!(root.query("*").unwrap().tag(), "box");
        assert_eq!(root.query_all("*").unwrap().len(), 3);
        assert_eq!(
            root.query("h2"),
            Err(QueryError::NotFound("h2".to_string()))
        );
        assert!(root.query("h1#nope").is_err());
    }

    #[test]
    fn query_errors() {
        let (root, ..) = tree();
        assert_eq!(root.query("  "), Err(QueryError::Empty));
        assert!(matches!(root.query("blink"), Err(QueryError::Invalid(_))));
        assert!(matches!(root.query("#"), Err(QueryError::Invalid(_))));
        assert!(matches!(root.query("p#a b"), Err(QueryError::Invalid(_))));
    }

    #[test]
    fn query_excludes_the_node_itself() {
        let (root, b, ..) = tree();
        assert!(b.query("box").is_err());
        assert!(root.query("box").is_ok());
    }

    #[test]
    fn snapshot_copies_the_subtree_and_ids() {
        let (root, b, h, _) = tree();
        let snap = root.snapshot_children();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].tag, "box");
        assert_eq!(snap[0].id, b.id());
        assert_eq!(snap[0].children[0].args, vec!["Title".to_string()]);
        assert_eq!(snap[0].children[0].id, h.id());
        // A snapshot does not follow later edits.
        h.set_arg(0, "Changed");
        assert_eq!(snap[0].children[0].args, vec!["Title".to_string()]);
    }

    #[test]
    fn text_collects_leaf_text() {
        let (root, b, h, _) = tree();
        assert_eq!(h.text(), "Title");
        assert_eq!(b.text(), "Title\nBody");
        let l = Node::create("link").unwrap();
        l.set_args(vec!["https://x.y".to_string(), "Home".to_string()]);
        assert_eq!(l.text(), "Home");
        assert_eq!(root.text(), "Title\nBody");
    }

    #[test]
    fn set_arg_pads_and_ids_are_unique() {
        let n = Node::create("link").unwrap();
        n.set_arg(1, "label");
        assert_eq!(n.args(), vec![String::new(), "label".to_string()]);
        let m = Node::create("p").unwrap();
        assert_ne!(n.id(), m.id());
    }
}
