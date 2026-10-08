//! Tokenizer for the cre language.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    /// "quoted", r#"raw"#, or a bare http(s):// url
    Str(String),
    Num(String),
    /// `#181818` (stored without the `#`)
    Hash(String),
    Colon,
    Dot,
    Comma,
    Eq,
    LBrace,
    RBrace,
    Let,
}

#[derive(Debug, Clone)]
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

    while i < c.len() {
        let ch = c[i];
        match ch {
            '\n' => {
                line += 1;
                i += 1;
            }
            ' ' | '\t' | '\r' => i += 1,
            '/' if c.get(i + 1) == Some(&'/') => {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
            }
            ':' => {
                out.push(Token {
                    tok: Tok::Colon,
                    line,
                });
                i += 1;
            }
            '.' => {
                out.push(Token {
                    tok: Tok::Dot,
                    line,
                });
                i += 1;
            }
            ',' => {
                out.push(Token {
                    tok: Tok::Comma,
                    line,
                });
                i += 1;
            }
            '=' => {
                out.push(Token { tok: Tok::Eq, line });
                i += 1;
            }
            '{' => {
                out.push(Token {
                    tok: Tok::LBrace,
                    line,
                });
                i += 1;
            }
            '}' => {
                out.push(Token {
                    tok: Tok::RBrace,
                    line,
                });
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
                    return Err(format!(
                        "line {}: `#` must be followed by a hex color",
                        line
                    ));
                }
                out.push(Token {
                    tok: Tok::Hash(s),
                    line,
                });
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
                                Some('"') => s.push('"'),
                                Some('\\') => s.push('\\'),
                                Some(o) => {
                                    return Err(format!("line {}: unknown escape `\\{}`", line, o));
                                }
                                None => {
                                    return Err(format!(
                                        "line {}: unterminated string",
                                        start_line
                                    ));
                                }
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
                out.push(Token {
                    tok: Tok::Str(s),
                    line: start_line,
                });
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
                out.push(Token {
                    tok: Tok::Str(s),
                    line: start_line,
                });
                i = j;
            }
            d if d.is_ascii_digit()
                || (d == '-' && c.get(i + 1).is_some_and(|x| x.is_ascii_digit())) =>
            {
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
                out.push(Token {
                    tok: Tok::Num(s),
                    line,
                });
            }
            a if a.is_alphabetic() || a == '_' => {
                let mut s = String::new();
                while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '-') {
                    s.push(c[i]);
                    i += 1;
                }
                if (s == "http" || s == "https") && c[i..].starts_with(&[':', '/', '/']) {
                    while i < c.len() && !c[i].is_whitespace() {
                        s.push(c[i]);
                        i += 1;
                    }
                    out.push(Token {
                        tok: Tok::Str(s),
                        line,
                    });
                } else if s == "let" {
                    out.push(Token {
                        tok: Tok::Let,
                        line,
                    });
                } else {
                    out.push(Token {
                        tok: Tok::Ident(s),
                        line,
                    });
                }
            }
            other => {
                return Err(format!(
                    "line {}: unexpected character `{}` (not implemented yet: selector rules like `body > *` and scripting)",
                    line, other
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_string_keeps_content() {
        let t = lex("r#\"a \"quoted\" b\"#").unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].tok, Tok::Str("a \"quoted\" b".to_string()));
    }

    #[test]
    fn bare_url_is_a_string() {
        let t = lex("link https://example.com \"Home\"").unwrap();
        assert_eq!(t[1].tok, Tok::Str("https://example.com".to_string()));
        assert_eq!(t[2].tok, Tok::Str("Home".to_string()));
    }

    #[test]
    fn style_modifier_tokens() {
        let toks: Vec<Tok> = lex("p.pad:1,2")
            .unwrap()
            .into_iter()
            .map(|t| t.tok)
            .collect();
        assert_eq!(
            toks,
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
}
