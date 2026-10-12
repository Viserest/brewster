//! Tokenizer for the `.cre` language: elements, styles and scripts.

mod bytes;
pub use bytes::{decode, encode};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    /// "quoted" (may interpolate `{expr}`) or a bare http(s):// url
    Str(String),
    /// r#"raw"# (never interpolates)
    Raw(String),
    Num(String),
    /// `#181818` (stored without the `#`)
    Hash(String),
    Colon,
    ColonColon,
    Dot,
    Comma,
    Semi,
    Eq,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// `=>`
    FatArrow,
    /// `->`
    Arrow,
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    AndAnd,
    OrOr,
    Pipe,
    Let,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
}

fn is_raw_start(c: &[char], i: usize) -> bool {
    let mut j = i + 1;
    while j < c.len() && c[j] == '#' {
        j += 1;
    }
    j < c.len() && c[j] == '"'
}

pub fn lex(src: &str) -> Result<Vec<Token>, String> {
    let c: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut line = 1;
    let mut out: Vec<Token> = Vec::new();
    let mut push = |tok: Tok, line: usize| out.push(Token { tok, line });

    while i < c.len() {
        let ch = c[i];
        let next = c.get(i + 1).copied();
        match ch {
            '\n' => {
                line += 1;
                i += 1;
            }
            ' ' | '\t' | '\r' => i += 1,
            '/' if next == Some('/') => {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
            }
            '/' => {
                push(Tok::Slash, line);
                i += 1;
            }
            ':' if next == Some(':') => {
                push(Tok::ColonColon, line);
                i += 2;
            }
            ':' => {
                push(Tok::Colon, line);
                i += 1;
            }
            '.' => {
                push(Tok::Dot, line);
                i += 1;
            }
            ',' => {
                push(Tok::Comma, line);
                i += 1;
            }
            ';' => {
                push(Tok::Semi, line);
                i += 1;
            }
            '=' if next == Some('=') => {
                push(Tok::EqEq, line);
                i += 2;
            }
            '=' if next == Some('>') => {
                push(Tok::FatArrow, line);
                i += 2;
            }
            '=' => {
                push(Tok::Eq, line);
                i += 1;
            }
            '!' if next == Some('=') => {
                push(Tok::Ne, line);
                i += 2;
            }
            '!' => {
                push(Tok::Bang, line);
                i += 1;
            }
            '<' if next == Some('=') => {
                push(Tok::Le, line);
                i += 2;
            }
            '<' => {
                push(Tok::Lt, line);
                i += 1;
            }
            '>' if next == Some('=') => {
                push(Tok::Ge, line);
                i += 2;
            }
            '>' => {
                push(Tok::Gt, line);
                i += 1;
            }
            '&' if next == Some('&') => {
                push(Tok::AndAnd, line);
                i += 2;
            }
            '|' if next == Some('|') => {
                push(Tok::OrOr, line);
                i += 2;
            }
            '|' => {
                push(Tok::Pipe, line);
                i += 1;
            }
            '-' if next == Some('>') => {
                push(Tok::Arrow, line);
                i += 2;
            }
            '-' => {
                push(Tok::Minus, line);
                i += 1;
            }
            '+' => {
                push(Tok::Plus, line);
                i += 1;
            }
            '*' => {
                push(Tok::Star, line);
                i += 1;
            }
            '%' => {
                push(Tok::Percent, line);
                i += 1;
            }
            '{' => {
                push(Tok::LBrace, line);
                i += 1;
            }
            '}' => {
                push(Tok::RBrace, line);
                i += 1;
            }
            '(' => {
                push(Tok::LParen, line);
                i += 1;
            }
            ')' => {
                push(Tok::RParen, line);
                i += 1;
            }
            '[' => {
                push(Tok::LBracket, line);
                i += 1;
            }
            ']' => {
                push(Tok::RBracket, line);
                i += 1;
            }
            '#' => {
                let mut j = i + 1;
                let mut s = String::new();
                while j < c.len() && c[j].is_ascii_alphanumeric() {
                    s.push(c[j]);
                    j += 1;
                }
                if s.is_empty() {
                    return Err(format!("line {}: `#` must be followed by a hex color", line));
                }
                push(Tok::Hash(s), line);
                i = j;
            }
            '"' => {
                let start_line = line;
                let mut s = String::new();
                i += 1;
                loop {
                    if i >= c.len() {
                        return Err(format!("line {}: unterminated string", start_line));
                    }
                    match c[i] {
                        '"' => {
                            i += 1;
                            break;
                        }
                        '\\' => {
                            i += 1;
                            match c.get(i).copied() {
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some('r') => s.push('\r'),
                                Some('"') => s.push('"'),
                                Some('\\') => s.push('\\'),
                                Some(o) => return Err(format!("line {}: unknown escape `\\{}`", line, o)),
                                None => return Err(format!("line {}: unterminated string", start_line)),
                            }
                            i += 1;
                        }
                        '\n' => {
                            line += 1;
                            s.push('\n');
                            i += 1;
                        }
                        o => {
                            s.push(o);
                            i += 1;
                        }
                    }
                }
                push(Tok::Str(s), start_line);
            }
            'r' if is_raw_start(&c, i) => {
                let start_line = line;
                let mut j = i + 1;
                let mut h = 0usize;
                while c[j] == '#' {
                    h += 1;
                    j += 1;
                }
                j += 1; // opening quote
                let mut s = String::new();
                loop {
                    if j >= c.len() {
                        return Err(format!("line {}: unterminated raw string", start_line));
                    }
                    if c[j] == '"' && (0..h).all(|k| c.get(j + 1 + k) == Some(&'#')) {
                        j += 1 + h;
                        break;
                    }
                    if c[j] == '\n' {
                        line += 1;
                    }
                    s.push(c[j]);
                    j += 1;
                }
                push(Tok::Raw(s), start_line);
                i = j;
            }
            d if d.is_ascii_digit() => {
                let mut s = String::new();
                s.push(d);
                i += 1;
                while i < c.len() && c[i].is_ascii_digit() {
                    s.push(c[i]);
                    i += 1;
                }
                if i + 1 < c.len() && c[i] == '.' && c[i + 1].is_ascii_digit() {
                    s.push('.');
                    i += 1;
                    while i < c.len() && c[i].is_ascii_digit() {
                        s.push(c[i]);
                        i += 1;
                    }
                }
                push(Tok::Num(s), line);
            }
            a if a.is_alphabetic() || a == '_' => {
                let mut s = String::new();
                while i < c.len() {
                    let x = c[i];
                    let dash_ok = x == '-' && c.get(i + 1).is_some_and(|n| n.is_alphabetic());
                    if x.is_alphanumeric() || x == '_' || dash_ok {
                        s.push(x);
                        i += 1;
                    } else {
                        break;
                    }
                }
                if (s == "http" || s == "https") && c[i..].starts_with(&[':', '/', '/']) {
                    while i < c.len() && !c[i].is_whitespace() {
                        s.push(c[i]);
                        i += 1;
                    }
                    push(Tok::Str(s), line);
                } else if s == "let" {
                    push(Tok::Let, line);
                } else {
                    push(Tok::Ident(s), line);
                }
            }
            other => {
                return Err(format!("line {}: unexpected character `{}`", line, other));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        lex(s).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn raw_string_keeps_content_and_is_distinct() {
        assert_eq!(toks("r#\"a \"quoted\" b\"#"), vec![Tok::Raw("a \"quoted\" b".to_string())]);
        assert_eq!(toks("\"x\""), vec![Tok::Str("x".to_string())]);
    }

    #[test]
    fn bare_url_is_a_string() {
        let t = lex("link https://example.com \"Home\"").unwrap();
        assert_eq!(t[1].tok, Tok::Str("https://example.com".to_string()));
        assert_eq!(t[2].tok, Tok::Str("Home".to_string()));
    }

    #[test]
    fn style_modifier_tokens() {
        assert_eq!(
            toks("p.pad:1,2"),
            vec![
                Tok::Ident("p".to_string()),
                Tok::Dot,
                Tok::Ident("pad".to_string()),
                Tok::Colon,
                Tok::Num("1".to_string()),
                Tok::Comma,
                Tok::Num("2".to_string()),
            ]
        );
    }

    #[test]
    fn dashed_names_stay_one_identifier_but_minus_is_an_operator() {
        assert_eq!(toks("align-x"), vec![Tok::Ident("align-x".to_string())]);
        assert_eq!(
            toks("a - 1"),
            vec![Tok::Ident("a".to_string()), Tok::Minus, Tok::Num("1".to_string())]
        );
        assert_eq!(
            toks("x-1"),
            vec![Tok::Ident("x".to_string()), Tok::Minus, Tok::Num("1".to_string())]
        );
    }

    #[test]
    fn numbers_and_dots() {
        assert_eq!(toks("3.14"), vec![Tok::Num("3.14".to_string())]);
        // A dot that is not followed by a digit is a separate token (`.border` after a value).
        assert_eq!(
            toks("0.border"),
            vec![Tok::Num("0".to_string()), Tok::Dot, Tok::Ident("border".to_string())]
        );
    }

    #[test]
    fn operators() {
        assert_eq!(
            toks("== != <= >= => -> :: && || | ! < > + * / %"),
            vec![
                Tok::EqEq,
                Tok::Ne,
                Tok::Le,
                Tok::Ge,
                Tok::FatArrow,
                Tok::Arrow,
                Tok::ColonColon,
                Tok::AndAnd,
                Tok::OrOr,
                Tok::Pipe,
                Tok::Bang,
                Tok::Lt,
                Tok::Gt,
                Tok::Plus,
                Tok::Star,
                Tok::Slash,
                Tok::Percent
            ]
        );
    }

    #[test]
    fn comments_and_lines() {
        let t = lex("a // x\nb").unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].line, 2);
    }

    #[test]
    fn errors() {
        assert!(lex("\"open").is_err());
        assert!(lex("r#\"open").is_err());
        assert!(lex("a @ b").is_err());
        assert!(lex("a & b").is_err());
        assert!(lex("#").is_err());
        assert!(lex("\"\\q\"").is_err());
    }

    #[test]
    fn let_is_a_keyword() {
        assert_eq!(toks("let x"), vec![Tok::Let, Tok::Ident("x".to_string())]);
    }
}
