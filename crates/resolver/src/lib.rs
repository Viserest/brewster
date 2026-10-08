//! Resolver: expands `let` templates, `.extends:`, `.before:` and `.after:`
//! into a flat tree where every node carries its final merged style.

use parser::Node;
use std::collections::{BTreeMap, HashMap};

pub type Style = BTreeMap<String, Vec<String>>;

#[derive(Debug, Clone)]
pub struct Resolved {
    pub tag: String,
    pub style: Style,
    pub args: Vec<String>,
    pub children: Vec<Resolved>,
}

/// Holds `let` templates. Templates must be defined before use.
#[derive(Debug, Default)]
pub struct Resolver {
    templates: HashMap<String, Node>,
}

impl Resolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn define(&mut self, name: String, node: Node) {
        self.templates.insert(name, node);
    }

    pub fn resolve(&self, node: &Node) -> Result<Resolved, String> {
        resolve(node, &self.templates, 0)
    }
}

/// Style plus before/after content for a node, following its `extends` chain.
/// A template's own children are deliberately not included.
fn layers(
    node: &Node,
    env: &HashMap<String, Node>,
    depth: usize,
) -> Result<(Style, Vec<Resolved>, Vec<Resolved>), String> {
    if depth > 32 {
        return Err(format!(
            "line {}: `extends` nesting too deep (cycle?)",
            node.line
        ));
    }
    let mut style = Style::new();
    let mut before: Vec<Resolved> = Vec::new();
    let mut after: Vec<Resolved> = Vec::new();

    if let Some((_, v)) = node.mods.iter().find(|(k, _)| k == "extends") {
        let name = v
            .first()
            .ok_or_else(|| format!("line {}: `extends` needs a name", node.line))?;
        let tpl = env.get(name).ok_or_else(|| {
            format!(
                "line {}: unknown template `{}` (define it with `let` first)",
                node.line, name
            )
        })?;
        let (s, b, a) = layers(tpl, env, depth + 1)?;
        style = s;
        before = b;
        after = a;
    }
    for (k, v) in &node.mods {
        if k != "extends" {
            style.insert(k.clone(), v.clone());
        }
    }
    if let Some(b) = &node.before {
        before.push(resolve(b, env, depth + 1)?);
    }
    if let Some(a) = &node.after {
        let mut merged = vec![resolve(a, env, depth + 1)?];
        merged.extend(after);
        after = merged;
    }
    Ok((style, before, after))
}

fn resolve(node: &Node, env: &HashMap<String, Node>, depth: usize) -> Result<Resolved, String> {
    let (style, before, after) = layers(node, env, depth)?;
    let mut children = before;
    for c in &node.children {
        children.push(resolve(c, env, depth + 1)?);
    }
    children.extend(after);
    Ok(Resolved {
        tag: node.tag.clone(),
        style,
        args: node.args.clone(),
        children,
    })
}
