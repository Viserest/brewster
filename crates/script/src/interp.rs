//! The interpreter: evaluates a program into a [`Node`] tree.
//!
//! Element statements append themselves to the enclosing element (or to the document at the
//! top level). Function bodies and `let` initialisers return their tail value instead, so
//! `fn card(t) { box { p t } }` returns the box. `let x = box ...` is a *template*: it is
//! built when used with `.extends:x` (style and `.before` / `.after` only) or `x.build()`.

use crate::methods;
use crate::value::*;

use std::{cell::RefCell, cmp::Ordering, rc::Rc};

use location::{Loader, Url};
use parser::{BinOp, Block, ElemNode, Expr, FnDef, Pat, Stmt, StmtKind, StrPart, UnOp};
use plugin::{PluginError, Registry};
use tree::{Node, Style};

/// What a page's scripts can reach besides the tree.
#[derive(Clone)]
pub struct Services {
    /// The page being run; its authority is the only application scope scripts can manage.
    pub url: Url,
    pub registry: Rc<RefCell<Registry>>,
    pub loader: Rc<dyn Loader>,
}

/// Evaluation steps one run (a page, a plugin, a click) may take.
pub const STEP_BUDGET: u64 = 2_000_000;
/// How much stack a script may use before it is stopped ("too deeply nested"). Frames are
/// large in debug builds, so this bounds recursion by memory, not by a call count; it fits
/// threads with the smallest common stack (1 MiB).
pub const STACK_BUDGET: usize = 512 * 1024;
const MAX_LOG: usize = 1000;
const MAX_TEMPLATE_DEPTH: usize = 32;

pub(crate) enum Signal {
    Return(Value),
    Error(String),
}

pub(crate) type R<T> = Result<T, Signal>;

pub struct Interpreter {
    pub(crate) global: Scope,
    pub(crate) root: Node,
    pub(crate) svc: Services,
    fuel: u64,
    stack_base: usize,
    stack_budget: usize,
    line: usize,
    log: Vec<String>,
}

impl Interpreter {
    pub fn new(root: Node, svc: Services) -> Interpreter {
        Interpreter {
            global: Scope::new_global(),
            root,
            svc,
            fuel: STEP_BUDGET,
            stack_base: stack_mark(),
            stack_budget: STACK_BUDGET,
            line: 0,
            log: Vec::new(),
        }
    }

    pub fn root(&self) -> &Node {
        &self.root
    }

    /// Changes how much stack scripts may use (default [`STACK_BUDGET`]). Raise it only
    /// when the interpreter runs on a thread with a larger stack.
    pub fn set_stack_budget(&mut self, bytes: usize) {
        self.stack_budget = bytes;
    }

    pub fn services(&self) -> &Services {
        &self.svc
    }

    /// Runs top-level statements: functions and variables stay defined for later calls,
    /// element statements are appended to the document.
    pub fn run(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        self.fuel = STEP_BUDGET;
        self.stack_base = stack_mark();
        match self.exec_program(stmts) {
            Ok(()) => Ok(()),
            Err(Signal::Error(e)) => Err(e),
            Err(Signal::Return(_)) => Ok(()),
        }
    }

    /// Calls a function defined by the page or a plugin.
    pub fn call(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        self.fuel = STEP_BUDGET;
        self.stack_base = stack_mark();
        let Some(f) = self.global.get(name) else {
            return Err(format!(
                "no function `{}` (define it in the page or in a plugin)",
                name
            ));
        };
        match self.call_value(&f, args) {
            Ok(v) => Ok(v),
            Err(Signal::Error(e)) => Err(e),
            Err(Signal::Return(v)) => Ok(v),
        }
    }

    /// Runs the `onclick:name(args)` function of a node. It receives the node first, then
    /// the literal arguments.
    pub fn click(&mut self, node: &Node) -> Result<(), String> {
        let Some(spec) = node.style_get("onclick") else {
            return Ok(());
        };
        let Some(name) = spec.first() else {
            return Err("`onclick` needs a function name".to_string());
        };
        let mut args = vec![Value::Node(node.clone())];
        for a in &spec[1..] {
            args.push(typed_literal(a)?);
        }
        self.call(name, args).map(|_| ())
    }

    /// Lines written by `log(...)` since the last call.
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.log)
    }

    // ── errors and limits ───────────────────────────────────────────

    pub(crate) fn rt(&self, msg: impl AsRef<str>) -> Signal {
        Signal::Error(format!("line {}: {}", self.line, msg.as_ref()))
    }

    fn check_stack(&self) -> R<()> {
        if stack_mark().abs_diff(self.stack_base) > self.stack_budget {
            return Err(self.rt("expressions nest or recurse too deeply"));
        }
        Ok(())
    }

    fn tick(&mut self) -> R<()> {
        if self.fuel == 0 {
            return Err(self.rt("the script ran too long and was stopped"));
        }
        self.fuel -= 1;
        Ok(())
    }

    pub(crate) fn push_log(&mut self, line: String) {
        if self.log.len() < MAX_LOG {
            self.log.push(line);
        }
    }

    // ── programs and blocks ─────────────────────────────────────────

    pub(crate) fn exec_program(&mut self, stmts: &[Stmt]) -> R<()> {
        let g = self.global.clone();
        let root = self.root.clone();
        match self.exec_stmts(stmts, &g, Some(&root)) {
            Ok(_) => Ok(()),
            Err(Signal::Return(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn hoist(&self, stmts: &[Stmt], scope: &Scope) {
        for s in stmts {
            if let StmtKind::Fn(f) = &s.kind {
                scope.define(&f.name, make_fn(f, scope), false);
            }
        }
    }

    /// Runs statements in `scope`. With a `sink`, element statements append to it and the
    /// block yields `()`; without one the value of a final expression is the block's value.
    pub(crate) fn exec_stmts(
        &mut self,
        stmts: &[Stmt],
        scope: &Scope,
        sink: Option<&Node>,
    ) -> R<Value> {
        self.hoist(stmts, scope);
        let mut last = Value::Unit;
        let n = stmts.len();
        for (i, st) in stmts.iter().enumerate() {
            self.tick()?;
            self.line = st.line;
            last = Value::Unit;
            match &st.kind {
                StmtKind::Let {
                    name,
                    mutable,
                    init,
                } => {
                    let v = match init {
                        Expr::Element(e) => Value::Template(Rc::new(Template {
                            node: (**e).clone(),
                            scope: scope.clone(),
                        })),
                        other => self.eval(other, scope)?,
                    };
                    scope.define(name, v, *mutable);
                }
                StmtKind::Fn(_) | StmtKind::TypeAlias => {}
                StmtKind::Expr { expr, semi } => {
                    let v = self.eval_stmt_expr(expr, scope, sink)?;
                    if !*semi && i + 1 == n {
                        last = v;
                    }
                }
            }
        }
        Ok(last)
    }

    /// An expression in statement position. With a sink, elements (also inside `if`,
    /// `match`, `while` and bare blocks) are appended to it.
    fn eval_stmt_expr(&mut self, expr: &Expr, scope: &Scope, sink: Option<&Node>) -> R<Value> {
        let Some(sink) = sink else {
            return self.eval(expr, scope);
        };
        match expr {
            Expr::Element(e) => {
                let node = self.build_element(e, scope)?;
                self.append_to(sink, &node)?;
                Ok(Value::Unit)
            }
            Expr::If(cond, then, els) => {
                self.tick()?;
                if self.eval_bool(cond, scope)? {
                    self.exec_stmts(&then.stmts, &Scope::child(scope), Some(sink))?;
                } else if let Some(e) = els {
                    self.exec_stmts(&e.stmts, &Scope::child(scope), Some(sink))?;
                }
                Ok(Value::Unit)
            }
            Expr::While(cond, body) => {
                while self.eval_bool(cond, scope)? {
                    self.exec_stmts(&body.stmts, &Scope::child(scope), Some(sink))?;
                }
                Ok(Value::Unit)
            }
            Expr::Block(b) => {
                self.exec_stmts(&b.stmts, &Scope::child(scope), Some(sink))?;
                Ok(Value::Unit)
            }
            Expr::Match(subject, arms) => {
                let v = self.eval(subject, scope)?;
                let (arm_scope, body) = self.select_arm(&v, arms, scope)?;
                self.eval_stmt_expr(body, &arm_scope, Some(sink))?;
                Ok(Value::Unit)
            }
            other => self.eval(other, scope),
        }
    }

    fn append_to(&self, parent: &Node, child: &Node) -> R<()> {
        parent
            .append(child)
            .map(|_| ())
            .map_err(|e| self.rt(format!("{} (in <{}>)", e, parent.tag())))
    }

    fn select_arm<'a>(
        &self,
        v: &Value,
        arms: &'a [(Pat, Expr)],
        scope: &Scope,
    ) -> R<(Scope, &'a Expr)> {
        for (pat, body) in arms {
            let mut binds = Vec::new();
            if match_pat(pat, v, &mut binds) {
                let s = Scope::child(scope);
                for (n, val) in binds {
                    s.define(&n, val, false);
                }
                return Ok((s, body));
            }
        }
        Err(self.rt(format!("no `match` arm fits {}", v.display())))
    }

    // ── functions ───────────────────────────────────────────────────

    pub(crate) fn call_value(&mut self, f: &Value, args: Vec<Value>) -> R<Value> {
        let Value::Func(func) = f else {
            return Err(self.rt(format!("a {} is not a function", f.type_name())));
        };
        let func = func.clone();
        if func.params.len() != args.len() {
            return Err(self.rt(format!(
                "`{}` takes {} argument{} but {} {} given",
                func.name,
                func.params.len(),
                if func.params.len() == 1 { "" } else { "s" },
                args.len(),
                if args.len() == 1 { "was" } else { "were" }
            )));
        }
        let scope = Scope::child(&func.scope);
        for (p, a) in func.params.iter().zip(args) {
            scope.define(p, a, true);
        }
        let saved_line = self.line;
        let r = match &func.body {
            Body::Block(b) => self.exec_stmts(&b.stmts, &scope, None),
            Body::Expr(e) => self.eval(e, &scope),
        };
        self.line = saved_line;
        match r {
            Ok(v) => Ok(v),
            Err(Signal::Return(v)) => Ok(v),
            Err(e) => Err(e),
        }
    }

    // ── expressions ─────────────────────────────────────────────────

    pub(crate) fn eval(&mut self, e: &Expr, scope: &Scope) -> R<Value> {
        self.tick()?;
        self.check_stack()?;
        self.eval_inner(e, scope)
    }

    fn eval_bool(&mut self, e: &Expr, scope: &Scope) -> R<bool> {
        match self.eval(e, scope)? {
            Value::Bool(b) => Ok(b),
            other => Err(self.rt(format!(
                "expected a Bool condition, found a {}",
                other.type_name()
            ))),
        }
    }

    fn eval_inner(&mut self, e: &Expr, scope: &Scope) -> R<Value> {
        // Each arm with real work lives in its own function so this frame stays small:
        // the depth of script recursion is bounded by the thread's stack.
        match e {
            Expr::Unit => Ok(Value::Unit),
            Expr::Int(i) => Ok(Value::Int(*i)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Raw(s) => Ok(Value::Str(s.clone())),
            Expr::Str(parts) => self.eval_str(parts, scope),
            Expr::Ident(name) => self.eval_ident(name, scope),
            Expr::Path(p) => self.eval_path(p),
            Expr::List(items) => self.eval_list(items, scope),
            Expr::Unary(op, x) => self.eval_unary(*op, x, scope),
            Expr::Binary(op, l, r) => self.eval_binary(*op, l, r, scope),
            Expr::Call(callee, args) => self.eval_call(callee, args, scope),
            Expr::Field(obj, name) => self.eval_field(obj, name, scope),
            Expr::Assign(lhs, rhs) => self.eval_assign(lhs, rhs, scope),
            Expr::If(cond, then, els) => self.eval_if(cond, then, els.as_ref(), scope),
            Expr::Match(subject, arms) => self.eval_match(subject, arms, scope),
            Expr::While(cond, body) => self.eval_while(cond, body, scope),
            Expr::Closure(params, body) => Ok(Value::Func(Rc::new(Function {
                name: "closure".to_string(),
                params: params.clone(),
                body: Body::Expr((**body).clone()),
                scope: scope.clone(),
            }))),
            Expr::Block(b) => self.exec_stmts(&b.stmts, &Scope::child(scope), None),
            Expr::Return(v) => self.eval_return(v.as_deref(), scope),
            Expr::Element(el) => Ok(Value::Node(self.build_element(el, scope)?)),
        }
    }

    #[inline(never)]
    fn eval_str(&mut self, parts: &[StrPart], scope: &Scope) -> R<Value> {
        let mut s = String::new();
        for p in parts {
            match p {
                StrPart::Text(t) => s.push_str(t),
                StrPart::Expr(x) => {
                    let v = self.eval(x, scope)?;
                    s.push_str(&v.display());
                }
            }
        }
        Ok(Value::Str(s))
    }

    #[inline(never)]
    fn eval_ident(&mut self, name: &str, scope: &Scope) -> R<Value> {
        if let Some(v) = scope.get(name) {
            return Ok(v);
        }
        match name {
            "None" => Ok(Value::none()),
            "root" => Ok(Value::Root),
            _ => Err(self.rt(format!("unknown variable `{}`", name))),
        }
    }

    #[inline(never)]
    fn eval_path(&mut self, p: &[String]) -> R<Value> {
        match p {
            [t, name] if t == "Tag" => {
                if tree::is_tag(name) {
                    Ok(Value::Tag(name.clone()))
                } else {
                    Err(self.rt(format!("`Tag::{}` is not an element", name)))
                }
            }
            _ => Err(self.rt(format!("`{}` must be called with ( )", p.join("::")))),
        }
    }

    #[inline(never)]
    fn eval_list(&mut self, items: &[Expr], scope: &Scope) -> R<Value> {
        let mut out = Vec::with_capacity(items.len());
        for i in items {
            out.push(self.eval(i, scope)?);
        }
        Ok(Value::List(out))
    }

    #[inline(never)]
    fn eval_unary(&mut self, op: UnOp, x: &Expr, scope: &Scope) -> R<Value> {
        let v = self.eval(x, scope)?;
        match (op, v) {
            (UnOp::Neg, Value::Int(i)) => i
                .checked_neg()
                .map(Value::Int)
                .ok_or_else(|| self.rt("integer overflow")),
            (UnOp::Neg, Value::Float(f)) => Ok(Value::Float(-f)),
            (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
            (UnOp::Neg, o) => Err(self.rt(format!("cannot negate a {}", o.type_name()))),
            (UnOp::Not, o) => Err(self.rt(format!("cannot apply `!` to a {}", o.type_name()))),
        }
    }

    #[inline(never)]
    fn eval_binary(&mut self, op: BinOp, l: &Expr, r: &Expr, scope: &Scope) -> R<Value> {
        match op {
            BinOp::And => {
                if !self.eval_bool(l, scope)? {
                    return Ok(Value::Bool(false));
                }
                Ok(Value::Bool(self.eval_bool(r, scope)?))
            }
            BinOp::Or => {
                if self.eval_bool(l, scope)? {
                    return Ok(Value::Bool(true));
                }
                Ok(Value::Bool(self.eval_bool(r, scope)?))
            }
            _ => {
                let a = self.eval(l, scope)?;
                let b = self.eval(r, scope)?;
                self.binop(op, a, b)
            }
        }
    }

    #[inline(never)]
    fn eval_field(&mut self, obj: &Expr, name: &str, scope: &Scope) -> R<Value> {
        let recv = self.eval(obj, scope)?;
        methods::field(self, &recv, name)
    }

    #[inline(never)]
    fn eval_assign(&mut self, lhs: &Expr, rhs: &Expr, scope: &Scope) -> R<Value> {
        let v = self.eval(rhs, scope)?;
        match lhs {
            Expr::Ident(n) => match scope.assign(n, v) {
                Ok(()) => Ok(Value::Unit),
                Err(AssignError::Immutable) => Err(self.rt(format!(
                    "cannot assign to `{}` because it is not `let mut`",
                    n
                ))),
                Err(AssignError::Unknown) => Err(self.rt(format!("unknown variable `{}`", n))),
            },
            _ => Err(self.rt(
                "only variables can be assigned; use `set_text`, `set_style`, ... on elements",
            )),
        }
    }

    #[inline(never)]
    fn eval_if(
        &mut self,
        cond: &Expr,
        then: &Block,
        els: Option<&Block>,
        scope: &Scope,
    ) -> R<Value> {
        if self.eval_bool(cond, scope)? {
            self.exec_stmts(&then.stmts, &Scope::child(scope), None)
        } else if let Some(e) = els {
            self.exec_stmts(&e.stmts, &Scope::child(scope), None)
        } else {
            Ok(Value::Unit)
        }
    }

    #[inline(never)]
    fn eval_match(&mut self, subject: &Expr, arms: &[(Pat, Expr)], scope: &Scope) -> R<Value> {
        let v = self.eval(subject, scope)?;
        let (arm_scope, body) = self.select_arm(&v, arms, scope)?;
        self.eval(body, &arm_scope)
    }

    #[inline(never)]
    fn eval_while(&mut self, cond: &Expr, body: &Block, scope: &Scope) -> R<Value> {
        while self.eval_bool(cond, scope)? {
            self.exec_stmts(&body.stmts, &Scope::child(scope), None)?;
        }
        Ok(Value::Unit)
    }

    #[inline(never)]
    fn eval_return(&mut self, v: Option<&Expr>, scope: &Scope) -> R<Value> {
        let val = match v {
            Some(x) => self.eval(x, scope)?,
            None => Value::Unit,
        };
        Err(Signal::Return(val))
    }

    fn eval_args(&mut self, args: &[Expr], scope: &Scope) -> R<Vec<Value>> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            out.push(self.eval(a, scope)?);
        }
        Ok(out)
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Expr], scope: &Scope) -> R<Value> {
        match callee {
            Expr::Ident(name) => {
                let argv = self.eval_args(args, scope)?;
                if let Some(f) = scope.get(name) {
                    return self.call_value(&f, argv);
                }
                methods::builtin(self, name, argv)
            }
            Expr::Path(p) => {
                let argv = self.eval_args(args, scope)?;
                methods::path_call(self, p, argv)
            }
            Expr::Field(obj, name) => {
                let recv = self.eval(obj, scope)?;
                let argv = self.eval_args(args, scope)?;
                methods::method(self, recv, name, argv)
            }
            other => {
                let f = self.eval(other, scope)?;
                let argv = self.eval_args(args, scope)?;
                self.call_value(&f, argv)
            }
        }
    }

    fn binop(&self, op: BinOp, a: Value, b: Value) -> R<Value> {
        use Value::*;
        match op {
            BinOp::Eq => Ok(Bool(values_equal(&a, &b))),
            BinOp::Ne => Ok(Bool(!values_equal(&a, &b))),
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let ord = compare(&a, &b).ok_or_else(|| {
                    self.rt(format!(
                        "cannot compare a {} with a {}",
                        a.type_name(),
                        b.type_name()
                    ))
                })?;
                Ok(Bool(match op {
                    BinOp::Lt => ord == Ordering::Less,
                    BinOp::Le => ord != Ordering::Greater,
                    BinOp::Gt => ord == Ordering::Greater,
                    _ => ord != Ordering::Less,
                }))
            }
            BinOp::Add => match (&a, &b) {
                (Int(x), Int(y)) => x
                    .checked_add(*y)
                    .map(Int)
                    .ok_or_else(|| self.rt("integer overflow")),
                (Int(_) | Float(_), Int(_) | Float(_)) => Ok(Float(as_f64(&a) + as_f64(&b))),
                (Str(x), _) => Ok(Str(format!("{}{}", x, b.display()))),
                (_, Str(y)) => Ok(Str(format!("{}{}", a.display(), y))),
                (List(x), List(y)) => Ok(List(x.iter().chain(y).cloned().collect())),
                _ => Err(self.rt(format!(
                    "cannot add a {} and a {}",
                    a.type_name(),
                    b.type_name()
                ))),
            },
            BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => self.arith(op, &a, &b),
            BinOp::And | BinOp::Or => unreachable!("short-circuit operators are handled earlier"),
        }
    }

    fn arith(&self, op: BinOp, a: &Value, b: &Value) -> R<Value> {
        use Value::*;
        let sym = match op {
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            _ => "%",
        };
        match (a, b) {
            (Int(x), Int(y)) => {
                if matches!(op, BinOp::Div | BinOp::Rem) && *y == 0 {
                    return Err(self.rt("division by zero"));
                }
                let r = match op {
                    BinOp::Sub => x.checked_sub(*y),
                    BinOp::Mul => x.checked_mul(*y),
                    BinOp::Div => x.checked_div(*y),
                    _ => x.checked_rem(*y),
                };
                r.map(Int).ok_or_else(|| self.rt("integer overflow"))
            }
            (Int(_) | Float(_), Int(_) | Float(_)) => {
                let (x, y) = (as_f64(a), as_f64(b));
                if matches!(op, BinOp::Div | BinOp::Rem) && y == 0.0 {
                    return Err(self.rt("division by zero"));
                }
                Ok(Float(match op {
                    BinOp::Sub => x - y,
                    BinOp::Mul => x * y,
                    BinOp::Div => x / y,
                    _ => x % y,
                }))
            }
            _ => Err(self.rt(format!(
                "cannot use `{}` on a {} and a {}",
                sym,
                a.type_name(),
                b.type_name()
            ))),
        }
    }

    // ── elements ────────────────────────────────────────────────────

    /// Style plus `.before` / `.after` content of an element, following `.extends:`. A
    /// template's own children are not included.
    fn layers(
        &mut self,
        e: &ElemNode,
        scope: &Scope,
        depth: usize,
    ) -> R<(Style, Vec<Node>, Vec<Node>)> {
        if depth > MAX_TEMPLATE_DEPTH {
            return Err(self.rt("`extends` nests too deeply (a cycle?)"));
        }
        let mut style = Style::new();
        let mut before: Vec<Node> = Vec::new();
        let mut after: Vec<Node> = Vec::new();

        if let Some((_, v)) = e.mods.iter().find(|(k, _)| k == "extends") {
            self.line = e.line;
            let Some(name) = v.first() else {
                return Err(self.rt("`extends` needs a template name"));
            };
            let Some(found) = scope.get(name) else {
                return Err(self.rt(format!(
                    "unknown template `{}` (define it with `let` before using it)",
                    name
                )));
            };
            let Value::Template(tpl) = found else {
                return Err(self.rt(format!(
                    "`{}` is a {}, not a template",
                    name,
                    found.type_name()
                )));
            };
            let (s, b, a) = self.layers(&tpl.node, &tpl.scope, depth + 1)?;
            style = s;
            before = b;
            after = a;
        }
        for (k, v) in &e.mods {
            if k != "extends" {
                style.insert(k.clone(), v.clone());
            }
        }
        if let Some(b) = &e.before {
            before.push(self.build_element_at(b, scope, depth + 1)?);
        }
        if let Some(a) = &e.after {
            let mut merged = vec![self.build_element_at(a, scope, depth + 1)?];
            merged.extend(after);
            after = merged;
        }
        Ok((style, before, after))
    }

    pub(crate) fn build_element(&mut self, e: &ElemNode, scope: &Scope) -> R<Node> {
        self.build_element_at(e, scope, 0)
    }

    fn build_element_at(&mut self, e: &ElemNode, scope: &Scope, depth: usize) -> R<Node> {
        self.tick()?;
        self.check_stack()?;
        self.build_element_inner(e, scope, depth)
    }

    fn build_element_inner(&mut self, e: &ElemNode, scope: &Scope, depth: usize) -> R<Node> {
        self.line = e.line;
        let (style, before, after) = self.layers(e, scope, depth)?;
        self.line = e.line;
        let node = Node::create(&e.tag).map_err(|er| self.rt(er.to_string()))?;
        node.set_style_map(style);
        let mut args = Vec::with_capacity(e.args.len());
        for a in &e.args {
            args.push(self.eval(a, scope)?.display());
        }
        node.set_args(args);
        for b in &before {
            self.append_to(&node, b)?;
        }
        if let Some(body) = &e.body {
            if node.tag() != "box" {
                return Err(self.rt(format!(
                    "`{}` cannot have a body; only `box` can hold elements",
                    e.tag
                )));
            }
            self.exec_stmts(&body.stmts, &Scope::child(scope), Some(&node))?;
        }
        for a in &after {
            self.append_to(&node, a)?;
        }
        Ok(node)
    }

    /// Builds the element a template describes.
    pub(crate) fn build_template(&mut self, t: &Template) -> R<Node> {
        self.build_element(&t.node, &t.scope)
    }

    // ── plugins ─────────────────────────────────────────────────────

    /// Runs a plugin's program now, in the page's global scope.
    pub(crate) fn run_plugin_now(&mut self, p: &plugin::Plugin) -> Result<(), PluginError> {
        let prog = p.program(&self.svc.url, &*self.svc.loader)?;
        match self.exec_program(&prog) {
            Ok(()) => Ok(()),
            Err(Signal::Error(m)) => Err(PluginError::Runtime(m)),
            Err(Signal::Return(_)) => Ok(()),
        }
    }
}

/// An address near the top of the stack, to measure how much a script has used. The stack
/// grows one way on every supported platform, so the distance is what counts.
#[inline(never)]
fn stack_mark() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

fn make_fn(f: &FnDef, scope: &Scope) -> Value {
    Value::Func(Rc::new(Function {
        name: f.name.clone(),
        params: f.params.clone(),
        body: Body::Block(f.body.clone()),
        scope: scope.clone(),
    }))
}

fn as_f64(v: &Value) -> f64 {
    match v {
        Value::Int(i) => *i as f64,
        Value::Float(f) => *f,
        _ => 0.0,
    }
}

fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    use Value::*;
    match (a, b) {
        (Int(x), Int(y)) => Some(x.cmp(y)),
        (Int(_) | Float(_), Int(_) | Float(_)) => as_f64(a).partial_cmp(&as_f64(b)),
        (Str(x), Str(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

fn match_pat(p: &Pat, v: &Value, binds: &mut Vec<(String, Value)>) -> bool {
    match (p, v) {
        (Pat::Wild, _) => true,
        (Pat::Bind(n), _) => {
            binds.push((n.clone(), v.clone()));
            true
        }
        (Pat::Unit, Value::Unit) => true,
        (Pat::Int(i), Value::Int(j)) => i == j,
        (Pat::Str(s), Value::Str(t)) => s == t,
        (Pat::Bool(a), Value::Bool(b)) => a == b,
        (Pat::Variant(n, inner), Value::Option(o)) => match (n.as_str(), o, inner.as_slice()) {
            ("None", None, []) => true,
            ("Some", Some(x), [p1]) => match_pat(p1, x, binds),
            _ => false,
        },
        (Pat::Variant(n, inner), Value::Result(r)) => match (n.as_str(), r, inner.as_slice()) {
            ("Ok", Ok(x), [p1]) => match_pat(p1, x, binds),
            ("Err", Err(x), [p1]) => match_pat(p1, x, binds),
            _ => false,
        },
        // `Err(NotFound)`: an error is matched by its kind.
        (Pat::Variant(n, inner), Value::Error { kind, .. }) => inner.is_empty() && n == kind,
        _ => false,
    }
}

/// `s:text`, `n:5`, `b:true` from an `onclick:name(...)` argument list.
fn typed_literal(a: &str) -> Result<Value, String> {
    let (kind, rest) = a
        .split_once(':')
        .ok_or_else(|| format!("bad onclick argument `{}`", a))?;
    match kind {
        "s" => Ok(Value::Str(rest.to_string())),
        "n" => {
            if rest.contains('.') {
                rest.parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| format!("bad number `{}`", rest))
            } else {
                rest.parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| format!("bad number `{}`", rest))
            }
        }
        "b" => Ok(Value::Bool(rest == "true")),
        _ => Err(format!("bad onclick argument `{}`", a)),
    }
}
