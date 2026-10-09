//! Parser: tokens -> AST (`let` bindings and element trees).

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

#[derive(Debug, Clone)]
pub struct Node {
    pub tag: String,
    /// Inline style modifiers in source order. Flags (e.g. `.protected`) have empty values.
    pub mods: Vec<(String, Vec<String>)>,
    pub before: Option<Box<Node>>,
    pub after: Option<Box<Node>>,
    pub args: Vec<String>,
    pub children: Vec<Node>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(String, Node),
    Elem(Node),
}

pub fn parse(toks: Vec<Token>) -> Result<Vec<Stmt>, String> {
    Parser { toks, pos: 0 }.parse_program()
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.tok)
    }

    fn line(&self) -> usize {
        self.toks
            .get(self.pos)
            .or(self.toks.last())
            .map_or(0, |t| t.line)
    }

    fn next(&mut self) -> Option<Token> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
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

    fn expect(&mut self, t: Tok, what: &str) -> Result<(), String> {
        let line = self.line();
        match self.next() {
            Some(tk) if tk.tok == t => Ok(()),
            _ => Err(format!("line {}: expected {}", line, what)),
        }
    }

    fn parse_program(mut self) -> Result<Vec<Stmt>, String> {
        let mut out = Vec::new();
        while self.peek().is_some() {
            if self.peek() == Some(&Tok::Let) {
                self.next();
                let name = self.expect_ident("a name after `let`")?;
                self.expect(Tok::Eq, "`=` after the name")?;
                let node = self.parse_element()?;
                out.push(Stmt::Let(name, node));
            } else {
                out.push(Stmt::Elem(self.parse_element()?));
            }
        }
        Ok(out)
    }

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
            if self.peek() == Some(&Tok::Comma) {
                self.next();
            } else {
                break;
            }
        }
        Ok(v)
    }

    fn parse_element(&mut self) -> Result<Node, String> {
        let line = self.line();
        let tag =
            self.expect_ident("an element name (box, h1-h6, p, span, link, input, button, pre)")?;
        if !is_tag(&tag) {
            return Err(format!("line {}: unknown element `{}`", line, tag));
        }
        let mut node = Node {
            tag,
            mods: Vec::new(),
            before: None,
            after: None,
            args: Vec::new(),
            children: Vec::new(),
            line,
        };

        while self.peek() == Some(&Tok::Dot) {
            self.next();
            let key = self.expect_ident("a style key after `.`")?;
            if self.peek() != Some(&Tok::Colon) {
                node.mods.push((key, Vec::new()));
                continue;
            }
            self.next();
            if key == "before" || key == "after" {
                let el = Box::new(self.parse_element()?);
                if key == "before" {
                    node.before = Some(el);
                } else {
                    node.after = Some(el);
                }
            } else {
                let vals = self.parse_values()?;
                node.mods.push((key, vals));
            }
        }

        loop {
            match self.peek().cloned() {
                Some(Tok::Str(s)) => {
                    self.next();
                    node.args.push(s);
                }
                Some(Tok::Ident(s))
                    if node.tag == "input" && node.args.is_empty() && !is_tag(&s) =>
                {
                    self.next();
                    node.args.push(s);
                }
                _ => break,
            }
        }

        if self.peek() == Some(&Tok::LBrace) {
            self.next();
            loop {
                match self.peek().cloned() {
                    Some(Tok::RBrace) => {
                        self.next();
                        break;
                    }
                    None => {
                        return Err(format!(
                            "line {}: missing `}}` for the block opened at line {}",
                            self.line(),
                            line
                        ));
                    }
                    Some(_) => {
                        let child = self.parse_element()?;
                        node.children.push(child);
                    }
                }
            }
        }
        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_block() {
        let stmts = parse(lexer::lex("box { p \"x\" }").unwrap()).unwrap();
        assert_eq!(stmts.len(), 1);
        match &stmts[0] {
            Stmt::Elem(n) => {
                assert_eq!(n.tag, "box");
                assert_eq!(n.children.len(), 1);
                assert_eq!(n.children[0].args, vec!["x".to_string()]);
            }
            _ => panic!("expected an element"),
        }
    }

    #[test]
    fn rejects_unknown_element() {
        assert!(parse(lexer::lex("blink \"x\"").unwrap()).is_err());
    }
}
