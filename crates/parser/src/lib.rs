//! Parser: tokens -> syntax tree (see [`ast`]).

pub mod ast;
pub use ast::*;

use lexer::{Tok, Token};

pub fn is_tag(s: &str) -> bool {
    matches!(
        s,
        "box"
            | "link"
            | "p"
            | "span"
            | "input"
            | "button"
            | "pre"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
    )
}

const KEYWORDS: &[&str] = &[
    "fn", "if", "else", "match", "while", "return", "true", "false", "mut", "type",
];

/// Parses a whole program.
pub fn parse(toks: Vec<Token>) -> Result<Vec<Stmt>, String> {
    let mut p = Parser {
        toks,
        pos: 0,
        depth: 0,
    };
    let mut out = Vec::new();
    while p.peek().is_some() {
        if p.eat(&Tok::Semi) {
            continue;
        }
        out.push(p.parse_stmt()?);
    }
    Ok(out)
}

/// Lexes and parses source text.
pub fn parse_source(src: &str) -> Result<Vec<Stmt>, String> {
    parse(lexer::lex(src)?)
}

/// Parses a single expression (used for `{expr}` inside strings).
pub fn parse_expr_source(src: &str, line: usize) -> Result<Expr, String> {
    let toks = lexer::lex(src)
        .map_err(|e| format!("line {}: in string interpolation: {}", line, e))?
        .into_iter()
        .map(|t| Token { tok: t.tok, line })
        .collect();
    let mut p = Parser {
        toks,
        pos: 0,
        depth: 0,
    };
    let e = p.parse_expr()?;
    if p.peek().is_some() {
        return Err(format!("line {}: unexpected text in `{{{}}}`", line, src));
    }
    Ok(e)
}

/// How deeply expressions, blocks and elements may nest. Deeper input is an error rather
/// than a stack overflow.
const MAX_DEPTH: usize = 128;

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(format!("line {}: nesting is too deep", self.line()));
        }
        Ok(())
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    fn peek_at(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.pos + n).map(|t| &t.tok)
    }

    fn line(&self) -> usize {
        self.toks
            .get(self.pos)
            .or(self.toks.last())
            .map_or(0, |t| t.line)
    }

    fn prev_line(&self) -> usize {
        self.pos
            .checked_sub(1)
            .and_then(|i| self.toks.get(i))
            .map_or(0, |t| t.line)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, t: Tok, what: &str) -> Result<(), String> {
        let line = self.line();
        if self.eat(&t) {
            Ok(())
        } else {
            Err(format!("line {}: expected {}", line, what))
        }
    }

    fn peek_ident(&self) -> Option<&str> {
        match self.peek() {
            Some(Tok::Ident(s)) => Some(s),
            _ => None,
        }
    }

    fn eat_keyword(&mut self, kw: &str) -> bool {
        if self.peek_ident() == Some(kw) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<String, String> {
        let line = self.line();
        match self.next() {
            Some(Token {
                tok: Tok::Ident(s), ..
            }) => Ok(s),
            _ => Err(format!("line {}: expected {}", line, what)),
        }
    }

    fn expect_name(&mut self, what: &str) -> Result<String, String> {
        let line = self.line();
        let n = self.expect_ident(what)?;
        if KEYWORDS.contains(&n.as_str()) || is_tag(&n) {
            return Err(format!(
                "line {}: `{}` is reserved and cannot be used as {}",
                line, n, what
            ));
        }
        Ok(n)
    }

    // ── statements ──────────────────────────────────────────────────

    fn parse_block(&mut self) -> Result<Block, String> {
        self.enter()?;
        let r = self.parse_block_inner();
        self.depth -= 1;
        r
    }

    fn parse_block_inner(&mut self) -> Result<Block, String> {
        let open = self.line();
        self.expect(Tok::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        loop {
            if self.eat(&Tok::RBrace) {
                break;
            }
            if self.peek().is_none() {
                return Err(format!(
                    "line {}: missing `}}` for the block opened at line {}",
                    self.line(),
                    open
                ));
            }
            if self.eat(&Tok::Semi) {
                continue;
            }
            stmts.push(self.parse_stmt()?);
        }
        Ok(Block { stmts })
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        let line = self.line();
        let kind = match self.peek() {
            Some(Tok::Let) => {
                self.next();
                let mutable = self.eat_keyword("mut");
                let name = self.expect_name("a name after `let`")?;
                if self.eat(&Tok::Colon) {
                    self.skip_type()?;
                }
                self.expect(Tok::Eq, "`=` after the name")?;
                let init = self.parse_expr()?;
                self.eat(&Tok::Semi);
                StmtKind::Let {
                    name,
                    mutable,
                    init,
                }
            }
            Some(Tok::Ident(s)) if s == "fn" && matches!(self.peek_at(1), Some(Tok::Ident(_))) => {
                self.next();
                StmtKind::Fn(self.parse_fn(line)?)
            }
            Some(Tok::Ident(s))
                if s == "type" && matches!(self.peek_at(1), Some(Tok::Ident(_))) =>
            {
                self.next();
                self.expect_ident("a type name")?;
                self.expect(Tok::Eq, "`=` after the type name")?;
                self.skip_type()?;
                self.eat(&Tok::Semi);
                StmtKind::TypeAlias
            }
            _ => {
                let expr = self.parse_expr()?;
                let semi = self.eat(&Tok::Semi);
                StmtKind::Expr { expr, semi }
            }
        };
        Ok(Stmt { kind, line })
    }

    fn parse_fn(&mut self, line: usize) -> Result<FnDef, String> {
        let name = self.expect_name("a function name")?;
        self.expect(Tok::LParen, "`(` after the function name")?;
        let mut params = Vec::new();
        while !self.eat(&Tok::RParen) {
            if !params.is_empty() {
                self.expect(Tok::Comma, "`,` between parameters")?;
                if self.eat(&Tok::RParen) {
                    break;
                }
            }
            params.push(self.expect_name("a parameter name")?);
            if self.eat(&Tok::Colon) {
                self.skip_type()?;
            }
        }
        if self.eat(&Tok::Arrow) {
            self.skip_type()?;
        }
        let body = self.parse_block()?;
        Ok(FnDef {
            name,
            params,
            body,
            line,
        })
    }

    /// Types are written like Rust's but are not checked: skip over one.
    fn skip_type(&mut self) -> Result<(), String> {
        let line = self.line();
        self.expect_ident("a type")
            .map_err(|_| format!("line {}: expected a type", line))?;
        loop {
            if self.eat(&Tok::ColonColon) {
                self.expect_ident("a type name after `::`")?;
            } else if self.peek() == Some(&Tok::Lt) {
                self.next();
                loop {
                    self.skip_type()?;
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::Gt, "`>` to close the type")?;
            } else if self.peek() == Some(&Tok::LParen) {
                // `Fn(msg: String) -> Element`
                let mut depth = 0usize;
                loop {
                    match self.next().map(|t| t.tok) {
                        Some(Tok::LParen) => depth += 1,
                        Some(Tok::RParen) => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        Some(_) => {}
                        None => return Err(format!("line {}: unclosed `(` in a type", line)),
                    }
                }
                if self.eat(&Tok::Arrow) {
                    self.skip_type()?;
                }
            } else {
                return Ok(());
            }
        }
    }

    // ── expressions ─────────────────────────────────────────────────

    pub(crate) fn parse_expr(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let r = self.parse_expr_inner();
        self.depth -= 1;
        r
    }

    fn parse_expr_inner(&mut self) -> Result<Expr, String> {
        let lhs = self.parse_binary(0)?;
        if self.peek() == Some(&Tok::Eq) {
            let line = self.line();
            if !matches!(lhs, Expr::Ident(_) | Expr::Field(..)) {
                return Err(format!(
                    "line {}: the left side of `=` must be a variable or a field",
                    line
                ));
            }
            self.next();
            let rhs = self.parse_expr()?;
            return Ok(Expr::Assign(Box::new(lhs), Box::new(rhs)));
        }
        Ok(lhs)
    }

    fn binop(&self) -> Option<(BinOp, u8)> {
        Some(match self.peek()? {
            Tok::OrOr => (BinOp::Or, 1),
            Tok::AndAnd => (BinOp::And, 2),
            Tok::EqEq => (BinOp::Eq, 3),
            Tok::Ne => (BinOp::Ne, 3),
            Tok::Lt => (BinOp::Lt, 4),
            Tok::Le => (BinOp::Le, 4),
            Tok::Gt => (BinOp::Gt, 4),
            Tok::Ge => (BinOp::Ge, 4),
            Tok::Plus => (BinOp::Add, 5),
            Tok::Minus => (BinOp::Sub, 5),
            Tok::Star => (BinOp::Mul, 6),
            Tok::Slash => (BinOp::Div, 6),
            Tok::Percent => (BinOp::Rem, 6),
            _ => return None,
        })
    }

    fn parse_binary(&mut self, min: u8) -> Result<Expr, String> {
        let mut lhs = self.parse_unary()?;
        while let Some((op, prec)) = self.binop() {
            if prec < min {
                break;
            }
            self.next();
            let rhs = self.parse_binary(prec + 1)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let r = self.parse_unary_inner();
        self.depth -= 1;
        r
    }

    fn parse_unary_inner(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.next();
                Ok(Expr::Unary(UnOp::Neg, Box::new(self.parse_unary()?)))
            }
            Some(Tok::Bang) => {
                self.next();
                Ok(Expr::Unary(UnOp::Not, Box::new(self.parse_unary()?)))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut e = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Tok::Dot)
                    if matches!(self.peek_at(1), Some(Tok::Ident(_)))
                        && self.peek_at(2) == Some(&Tok::Colon) =>
                {
                    // `"text".after:span ...`: a style modifier of an element, not a field.
                    return Ok(e);
                }
                Some(Tok::Dot) => {
                    // An element consumes its own `.mods`; this is a field or method access.
                    self.next();
                    let name = self.expect_ident("a field or method name after `.`")?;
                    e = Expr::Field(Box::new(e), name);
                }
                Some(Tok::LParen) => {
                    self.next();
                    let mut args = Vec::new();
                    while !self.eat(&Tok::RParen) {
                        if !args.is_empty() {
                            self.expect(Tok::Comma, "`,` between arguments")?;
                            if self.eat(&Tok::RParen) {
                                break;
                            }
                        }
                        args.push(self.parse_expr()?);
                    }
                    e = Expr::Call(Box::new(e), args);
                }
                _ => return Ok(e),
            }
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        let line = self.line();
        let Some(tok) = self.peek().cloned() else {
            return Err(format!(
                "line {}: unexpected end of input, expected an expression",
                line
            ));
        };
        match tok {
            Tok::Num(s) => {
                self.next();
                if s.contains('.') {
                    s.parse::<f64>()
                        .map(Expr::Float)
                        .map_err(|_| format!("line {}: bad number `{}`", line, s))
                } else {
                    s.parse::<i64>()
                        .map(Expr::Int)
                        .map_err(|_| format!("line {}: number `{}` is too large", line, s))
                }
            }
            Tok::Str(s) => {
                self.next();
                Ok(Expr::Str(parse_interpolated(&s, line)?))
            }
            Tok::Raw(s) => {
                self.next();
                Ok(Expr::Raw(s))
            }
            Tok::LBracket => {
                self.next();
                let mut items = Vec::new();
                while !self.eat(&Tok::RBracket) {
                    if !items.is_empty() {
                        self.expect(Tok::Comma, "`,` between list items")?;
                        if self.eat(&Tok::RBracket) {
                            break;
                        }
                    }
                    items.push(self.parse_expr()?);
                }
                Ok(Expr::List(items))
            }
            Tok::LParen => {
                self.next();
                if self.eat(&Tok::RParen) {
                    return Ok(Expr::Unit);
                }
                let e = self.parse_expr()?;
                self.expect(Tok::RParen, "`)`")?;
                Ok(e)
            }
            Tok::LBrace => Ok(Expr::Block(self.parse_block()?)),
            Tok::Pipe | Tok::OrOr => self.parse_closure(),
            Tok::Ident(name) => self.parse_ident_expr(name, line),
            other => Err(format!(
                "line {}: unexpected {} in an expression",
                line,
                describe(&other)
            )),
        }
    }

    fn parse_closure(&mut self) -> Result<Expr, String> {
        let mut params = Vec::new();
        if self.eat(&Tok::OrOr) {
            // `||` is an empty parameter list
        } else {
            self.expect(Tok::Pipe, "`|`")?;
            while !self.eat(&Tok::Pipe) {
                if !params.is_empty() {
                    self.expect(Tok::Comma, "`,` between closure parameters")?;
                }
                params.push(self.expect_name("a closure parameter")?);
                if self.eat(&Tok::Colon) {
                    self.skip_type()?;
                }
            }
        }
        let body = if self.peek() == Some(&Tok::LBrace) {
            Expr::Block(self.parse_block()?)
        } else {
            self.parse_expr()?
        };
        Ok(Expr::Closure(params, Box::new(body)))
    }

    fn parse_ident_expr(&mut self, name: String, line: usize) -> Result<Expr, String> {
        match name.as_str() {
            "true" => {
                self.next();
                Ok(Expr::Bool(true))
            }
            "false" => {
                self.next();
                Ok(Expr::Bool(false))
            }
            "if" => {
                self.next();
                self.parse_if()
            }
            "match" => {
                self.next();
                self.parse_match()
            }
            "while" => {
                self.next();
                let cond = self.parse_expr()?;
                let body = self.parse_block()?;
                Ok(Expr::While(Box::new(cond), body))
            }
            "return" => {
                self.next();
                let has_value = match self.peek() {
                    None | Some(Tok::Semi) | Some(Tok::RBrace) => false,
                    _ => true,
                };
                let v = if has_value {
                    Some(Box::new(self.parse_expr()?))
                } else {
                    None
                };
                Ok(Expr::Return(v))
            }
            n if is_tag(n) => Ok(Expr::Element(Box::new(self.parse_element()?))),
            n if KEYWORDS.contains(&n) => Err(format!("line {}: unexpected keyword `{}`", line, n)),
            _ => {
                self.next();
                if self.peek() == Some(&Tok::ColonColon) {
                    let mut path = vec![name];
                    while self.eat(&Tok::ColonColon) {
                        path.push(self.expect_ident("a name after `::`")?);
                    }
                    Ok(Expr::Path(path))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
        }
    }

    fn parse_if(&mut self) -> Result<Expr, String> {
        let cond = self.parse_expr()?;
        let then = self.parse_block()?;
        let els = if self.eat_keyword("else") {
            if self.eat_keyword("if") {
                let line = self.prev_line();
                let nested = self.parse_if()?;
                Some(Block {
                    stmts: vec![Stmt {
                        kind: StmtKind::Expr {
                            expr: nested,
                            semi: false,
                        },
                        line,
                    }],
                })
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };
        Ok(Expr::If(Box::new(cond), then, els))
    }

    fn parse_match(&mut self) -> Result<Expr, String> {
        let subject = self.parse_expr()?;
        self.expect(Tok::LBrace, "`{` after the match subject")?;
        let mut arms = Vec::new();
        while !self.eat(&Tok::RBrace) {
            if self.peek().is_none() {
                return Err(format!(
                    "line {}: missing `}}` to close `match`",
                    self.line()
                ));
            }
            let pat = self.parse_pat()?;
            self.expect(Tok::FatArrow, "`=>` after the pattern")?;
            let body = if self.peek() == Some(&Tok::LBrace) {
                Expr::Block(self.parse_block()?)
            } else {
                self.parse_expr()?
            };
            arms.push((pat, body));
            self.eat(&Tok::Comma);
        }
        Ok(Expr::Match(Box::new(subject), arms))
    }

    fn parse_pat(&mut self) -> Result<Pat, String> {
        let line = self.line();
        match self.next().map(|t| t.tok) {
            Some(Tok::LParen) => {
                self.expect(Tok::RParen, "`)` (only `()` is a pattern in parentheses)")?;
                Ok(Pat::Unit)
            }
            Some(Tok::Num(s)) => s
                .parse::<i64>()
                .map(Pat::Int)
                .map_err(|_| format!("line {}: bad number pattern `{}`", line, s)),
            Some(Tok::Minus) => match self.next().map(|t| t.tok) {
                Some(Tok::Num(s)) => s
                    .parse::<i64>()
                    .map(|n| Pat::Int(-n))
                    .map_err(|_| format!("line {}: bad number pattern `-{}`", line, s)),
                _ => Err(format!(
                    "line {}: expected a number after `-` in a pattern",
                    line
                )),
            },
            Some(Tok::Str(s)) | Some(Tok::Raw(s)) => Ok(Pat::Str(s)),
            Some(Tok::Ident(n)) => match n.as_str() {
                "_" => Ok(Pat::Wild),
                "true" => Ok(Pat::Bool(true)),
                "false" => Ok(Pat::Bool(false)),
                _ if n.chars().next().is_some_and(|c| c.is_uppercase()) => {
                    let mut inner = Vec::new();
                    if self.eat(&Tok::LParen) {
                        while !self.eat(&Tok::RParen) {
                            if !inner.is_empty() {
                                self.expect(Tok::Comma, "`,` between patterns")?;
                            }
                            inner.push(self.parse_pat()?);
                        }
                    }
                    Ok(Pat::Variant(n, inner))
                }
                _ => Ok(Pat::Bind(n)),
            },
            _ => Err(format!("line {}: expected a pattern", line)),
        }
    }

    // ── elements ────────────────────────────────────────────────────

    fn parse_values(&mut self) -> Result<Vec<String>, String> {
        let mut v = Vec::new();
        loop {
            let line = self.line();
            match self.next() {
                Some(Token {
                    tok: Tok::Ident(s), ..
                })
                | Some(Token {
                    tok: Tok::Num(s), ..
                })
                | Some(Token {
                    tok: Tok::Str(s), ..
                }) => v.push(s),
                Some(Token {
                    tok: Tok::Hash(s), ..
                }) => v.push(format!("#{}", s)),
                _ => return Err(format!("line {}: expected a style value", line)),
            }
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        Ok(v)
    }

    /// `onclick:name` or `onclick:name("text", 5, true)`: the name, then typed literals.
    fn parse_onclick(&mut self) -> Result<Vec<String>, String> {
        let mut vals = vec![self.expect_ident("a function name after `onclick:`")?];
        if self.eat(&Tok::LParen) {
            while !self.eat(&Tok::RParen) {
                if vals.len() > 1 {
                    self.expect(Tok::Comma, "`,` between arguments")?;
                }
                let line = self.line();
                match self.next().map(|t| t.tok) {
                    Some(Tok::Str(s)) | Some(Tok::Raw(s)) => vals.push(format!("s:{}", s)),
                    Some(Tok::Num(s)) => vals.push(format!("n:{}", s)),
                    Some(Tok::Ident(s)) if s == "true" || s == "false" => {
                        vals.push(format!("b:{}", s))
                    }
                    _ => {
                        return Err(format!(
                            "line {}: `onclick` arguments must be literals (text, numbers, true or false)",
                            line
                        ));
                    }
                }
            }
        }
        Ok(vals)
    }

    fn parse_element(&mut self) -> Result<ElemNode, String> {
        self.parse_element_in(false)
    }

    /// `inner` is true for the element after `.before:` / `.after:`: a following
    /// `.before` / `.after` belongs to the outer element, not to this one.
    fn parse_element_in(&mut self, inner: bool) -> Result<ElemNode, String> {
        self.enter()?;
        let r = self.parse_element_inner(inner);
        self.depth -= 1;
        r
    }

    fn parse_element_inner(&mut self, inner: bool) -> Result<ElemNode, String> {
        let line = self.line();
        let tag =
            self.expect_ident("an element name (box, h1-h6, p, span, link, input, button, pre)")?;
        if !is_tag(&tag) {
            return Err(format!("line {}: unknown element `{}`", line, tag));
        }
        let mut node = ElemNode {
            tag,
            mods: Vec::new(),
            before: None,
            after: None,
            args: Vec::new(),
            body: None,
            line,
        };

        while self.peek() == Some(&Tok::Dot) {
            if inner
                && matches!(self.peek_at(1), Some(Tok::Ident(k)) if k == "before" || k == "after")
                && self.peek_at(2) == Some(&Tok::Colon)
            {
                break;
            }
            self.next();
            let key = self.expect_ident("a style key after `.`")?;
            if self.peek() != Some(&Tok::Colon) {
                node.mods.push((key, Vec::new()));
                continue;
            }
            self.next();
            match key.as_str() {
                "before" | "after" => {
                    let el = Box::new(self.parse_element_in(true)?);
                    if key == "before" {
                        node.before = Some(el);
                    } else {
                        node.after = Some(el);
                    }
                }
                "onclick" => {
                    let v = self.parse_onclick()?;
                    node.mods.push((key, v));
                }
                _ => {
                    let vals = self.parse_values()?;
                    node.mods.push((key, vals));
                }
            }
        }

        loop {
            match self.peek().cloned() {
                Some(Tok::Str(_)) | Some(Tok::Raw(_)) | Some(Tok::Num(_)) => {
                    node.args.push(self.parse_binary(0)?);
                }
                Some(Tok::Ident(s))
                    if !is_tag(&s)
                        && !KEYWORDS.contains(&s.as_str())
                        && self.line() == self.prev_line()
                        && self.peek_at(1) != Some(&Tok::FatArrow) =>
                {
                    if node.tag == "input" && node.args.is_empty() {
                        // `input username`: a bare name is the field's name, not a variable.
                        self.next();
                        node.args.push(Expr::Str(vec![StrPart::Text(s)]));
                    } else {
                        // `p text`, `h1 title.upper()`: a variable or call on the same line.
                        node.args.push(self.parse_binary(0)?);
                    }
                }
                _ => break,
            }
        }

        // Only a `box` holds children. After `.before:` / `.after:` a `{` therefore belongs
        // to the outer element when this one is a `p`, `span`, ... .
        let takes_body = !inner || node.tag == "box";
        if takes_body && self.peek() == Some(&Tok::LBrace) {
            node.body = Some(self.parse_block()?);
        }
        Ok(node)
    }
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Ident(s) => format!("`{}`", s),
        Tok::Str(_) | Tok::Raw(_) => "a string".to_string(),
        Tok::Num(n) => format!("`{}`", n),
        other => format!("{:?}", other),
    }
}

/// Splits a quoted string into text and `{expr}` parts. `{{` and `}}` are literal braces.
fn parse_interpolated(s: &str, line: usize) -> Result<Vec<StrPart>, String> {
    if !s.contains('{') && !s.contains('}') {
        return Ok(vec![StrPart::Text(s.to_string())]);
    }
    let c: Vec<char> = s.chars().collect();
    let mut parts = Vec::new();
    let mut text = String::new();
    let mut i = 0;
    while i < c.len() {
        match c[i] {
            '{' if c.get(i + 1) == Some(&'{') => {
                text.push('{');
                i += 2;
            }
            '}' if c.get(i + 1) == Some(&'}') => {
                text.push('}');
                i += 2;
            }
            '}' => {
                return Err(format!(
                    "line {}: unmatched `}}` in a string (write `}}}}` for a literal brace)",
                    line
                ));
            }
            '{' => {
                let mut depth = 1usize;
                let mut j = i + 1;
                while j < c.len() && depth > 0 {
                    match c[j] {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                if depth != 0 {
                    return Err(format!(
                        "line {}: missing `}}` in a string (write `{{{{` for a literal brace)",
                        line
                    ));
                }
                let inner: String = c[i + 1..j - 1].iter().collect();
                if inner.trim().is_empty() {
                    return Err(format!("line {}: empty `{{}}` in a string", line));
                }
                if !text.is_empty() {
                    parts.push(StrPart::Text(std::mem::take(&mut text)));
                }
                parts.push(StrPart::Expr(parse_expr_source(&inner, line)?));
                i = j;
            }
            ch => {
                text.push(ch);
                i += 1;
            }
        }
    }
    if !text.is_empty() {
        parts.push(StrPart::Text(text));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests;
