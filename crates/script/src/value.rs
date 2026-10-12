//! Runtime values and variable scopes.

use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};

use parser::{Block, ElemNode, Expr};
use plugin::{Plugin, Uuid};
use tree::Node;

#[derive(Clone)]
pub enum Value {
    Unit,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    List(Vec<Value>),
    Option(Option<Box<Value>>),
    Result(Result<Box<Value>, Box<Value>>),
    /// An error from the browser (`PluginError`, `QueryError`, `NodeError`, ...). `kind`
    /// is the variant name, so `Err(NotFound) => ...` can match on it.
    Error {
        kind: String,
        message: String,
    },
    Node(Node),
    Tag(String),
    Plugin(Plugin),
    Uuid(Uuid),
    Func(Rc<Function>),
    /// `let name = box ...`: an element description that is built when used.
    Template(Rc<Template>),
    /// `root`
    Root,
    /// `root.plugin`
    RootPlugin,
}

pub enum Body {
    Block(Block),
    Expr(Expr),
}

pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Body,
    pub scope: Scope,
}

pub struct Template {
    pub node: ElemNode,
    pub scope: Scope,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Unit => "Unit",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Bool(_) => "Bool",
            Value::Str(_) => "String",
            Value::List(_) => "List",
            Value::Option(_) => "Option",
            Value::Result(_) => "Result",
            Value::Error { .. } => "Error",
            Value::Node(_) => "Element",
            Value::Tag(_) => "Tag",
            Value::Plugin(_) => "Plugin",
            Value::Uuid(_) => "Uuid",
            Value::Func(_) => "Function",
            Value::Template(_) => "Template",
            Value::Root => "Root",
            Value::RootPlugin => "RootPlugin",
        }
    }

    pub fn error(kind: &str, message: impl Into<String>) -> Value {
        Value::Error {
            kind: kind.to_string(),
            message: message.into(),
        }
    }

    pub fn ok(v: Value) -> Value {
        Value::Result(Ok(Box::new(v)))
    }

    pub fn err(e: Value) -> Value {
        Value::Result(Err(Box::new(e)))
    }

    pub fn some(v: Value) -> Value {
        Value::Option(Some(Box::new(v)))
    }

    pub fn none() -> Value {
        Value::Option(None)
    }

    /// How a value reads inside a string or as element text.
    pub fn display(&self) -> String {
        match self {
            Value::Unit => "()".to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Str(s) => s.clone(),
            Value::List(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.display()).collect();
                format!("[{}]", parts.join(", "))
            }
            Value::Option(Some(v)) => v.display(),
            Value::Option(None) => "None".to_string(),
            Value::Result(Ok(v)) => format!("Ok({})", v.display()),
            Value::Result(Err(e)) => format!("Err({})", e.display()),
            Value::Error { message, .. } => message.clone(),
            Value::Node(n) => n.text(),
            Value::Tag(t) => t.clone(),
            Value::Plugin(p) => match p {
                Plugin::Compiled(b) => format!("<compiled plugin, {} bytes>", b.len()),
                Plugin::Source(_) => "<source plugin>".to_string(),
                Plugin::Url(u) => format!("<plugin at {}>", u),
            },
            Value::Uuid(u) => u.to_string(),
            Value::Func(f) => format!("<fn {}>", f.name),
            Value::Template(_) => "<template>".to_string(),
            Value::Root => "<root>".to_string(),
            Value::RootPlugin => "<root.plugin>".to_string(),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Str(s) => write!(f, "{:?}", s),
            Value::Node(n) => write!(f, "{:?}", n),
            Value::Error { kind, message } => write!(f, "{}({:?})", kind, message),
            other => write!(f, "{}", other.display()),
        }
    }
}

pub fn values_equal(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Unit, Unit) => true,
        (Int(x), Int(y)) => x == y,
        (Float(x), Float(y)) => x == y,
        (Int(x), Float(y)) | (Float(y), Int(x)) => (*x as f64) == *y,
        (Bool(x), Bool(y)) => x == y,
        (Str(x), Str(y)) => x == y,
        (Tag(x), Tag(y)) => x == y,
        (Tag(x), Str(y)) | (Str(y), Tag(x)) => x == y,
        (List(x), List(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| values_equal(p, q))
        }
        (Option(None), Option(None)) => true,
        (Option(Some(x)), Option(Some(y))) => values_equal(x, y),
        (Result(Ok(x)), Result(Ok(y))) | (Result(Err(x)), Result(Err(y))) => values_equal(x, y),
        (
            Error {
                kind: k1,
                message: m1,
            },
            Error {
                kind: k2,
                message: m2,
            },
        ) => k1 == k2 && m1 == m2,
        (Node(x), Node(y)) => x == y,
        (Uuid(x), Uuid(y)) => x == y,
        (Plugin(x), Plugin(y)) => x == y,
        _ => false,
    }
}

// ── scopes ──────────────────────────────────────────────────────────

struct ScopeData {
    vars: HashMap<String, (Value, bool)>,
    parent: Option<Scope>,
}

/// A chain of variable tables. Cloning shares the scope.
#[derive(Clone)]
pub struct Scope(Rc<RefCell<ScopeData>>);

#[derive(Debug, PartialEq, Eq)]
pub enum AssignError {
    Unknown,
    Immutable,
}

impl Scope {
    pub fn new_global() -> Scope {
        Scope(Rc::new(RefCell::new(ScopeData {
            vars: HashMap::new(),
            parent: None,
        })))
    }

    pub fn child(parent: &Scope) -> Scope {
        Scope(Rc::new(RefCell::new(ScopeData {
            vars: HashMap::new(),
            parent: Some(parent.clone()),
        })))
    }

    /// Defines (or shadows) a variable in this scope.
    pub fn define(&self, name: &str, value: Value, mutable: bool) {
        self.0
            .borrow_mut()
            .vars
            .insert(name.to_string(), (value, mutable));
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        let d = self.0.borrow();
        match d.vars.get(name) {
            Some((v, _)) => Some(v.clone()),
            None => d.parent.as_ref().and_then(|p| p.get(name)),
        }
    }

    pub fn assign(&self, name: &str, value: Value) -> Result<(), AssignError> {
        let mut d = self.0.borrow_mut();
        if let Some((slot, mutable)) = d.vars.get_mut(name) {
            if !*mutable {
                return Err(AssignError::Immutable);
            }
            *slot = value;
            return Ok(());
        }
        match &d.parent {
            Some(p) => p.assign(name, value),
            None => Err(AssignError::Unknown),
        }
    }
}
