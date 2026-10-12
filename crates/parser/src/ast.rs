//! Syntax tree of a `.cre` program: elements, styles and scripts share one grammar.

#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    Text(String),
    /// `{expr}` inside a quoted string.
    Expr(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pat {
    Wild,
    /// `()`
    Unit,
    Bind(String),
    Int(i64),
    Str(String),
    Bool(bool),
    /// `Some(p)`, `None`, `Ok(p)`, `Err(p)`.
    Variant(String, Vec<Pat>),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// `let [mut] name [: Type] = init`. When `init` is an element it names a template.
    Let {
        name: String,
        mutable: bool,
        init: Expr,
    },
    Fn(FnDef),
    /// An expression; `semi` is true when it was followed by `;` (its value is dropped).
    Expr { expr: Expr, semi: bool },
    /// `type Name = ...` is accepted and ignored.
    TypeAlias,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: Block,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// `()`
    Unit,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Vec<StrPart>),
    Raw(String),
    Ident(String),
    /// `Plugin::source`
    Path(Vec<String>),
    List(Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Field(Box<Expr>, String),
    Assign(Box<Expr>, Box<Expr>),
    If(Box<Expr>, Block, Option<Block>),
    Match(Box<Expr>, Vec<(Pat, Expr)>),
    While(Box<Expr>, Block),
    Closure(Vec<String>, Box<Expr>),
    Block(Block),
    Return(Option<Box<Expr>>),
    Element(Box<ElemNode>),
}

/// An element expression: `box.pad:1.extends:t "arg" { children }`.
#[derive(Debug, Clone, PartialEq)]
pub struct ElemNode {
    pub tag: String,
    /// Inline style modifiers in source order. Flags (e.g. `.protected`) have no values.
    /// `onclick` holds the function name followed by typed literal arguments
    /// (`s:text`, `n:5`, `b:true`).
    pub mods: Vec<(String, Vec<String>)>,
    pub before: Option<Box<ElemNode>>,
    pub after: Option<Box<ElemNode>>,
    pub args: Vec<Expr>,
    pub body: Option<Block>,
    pub line: usize,
}
