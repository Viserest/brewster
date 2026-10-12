//! Binary form of a token list: what a "compiled" plugin is made of.
//!
//! Layout: magic `CREB`, version byte, then for every token a kind byte, the line (u32,
//! little endian) and, for tokens that carry text, a u32 length and the UTF-8 bytes.

use crate::{Tok, Token};

const MAGIC: &[u8; 4] = b"CREB";
const VERSION: u8 = 1;

fn kind_of(t: &Tok) -> (u8, Option<&str>) {
    match t {
        Tok::Ident(s) => (0, Some(s)),
        Tok::Str(s) => (1, Some(s)),
        Tok::Raw(s) => (2, Some(s)),
        Tok::Num(s) => (3, Some(s)),
        Tok::Hash(s) => (4, Some(s)),
        Tok::Colon => (10, None),
        Tok::ColonColon => (11, None),
        Tok::Dot => (12, None),
        Tok::Comma => (13, None),
        Tok::Semi => (14, None),
        Tok::Eq => (15, None),
        Tok::EqEq => (16, None),
        Tok::Ne => (17, None),
        Tok::Lt => (18, None),
        Tok::Le => (19, None),
        Tok::Gt => (20, None),
        Tok::Ge => (21, None),
        Tok::FatArrow => (22, None),
        Tok::Arrow => (23, None),
        Tok::LBrace => (24, None),
        Tok::RBrace => (25, None),
        Tok::LParen => (26, None),
        Tok::RParen => (27, None),
        Tok::LBracket => (28, None),
        Tok::RBracket => (29, None),
        Tok::Plus => (30, None),
        Tok::Minus => (31, None),
        Tok::Star => (32, None),
        Tok::Slash => (33, None),
        Tok::Percent => (34, None),
        Tok::Bang => (35, None),
        Tok::AndAnd => (36, None),
        Tok::OrOr => (37, None),
        Tok::Pipe => (38, None),
        Tok::Let => (39, None),
    }
}

fn plain(kind: u8) -> Option<Tok> {
    Some(match kind {
        10 => Tok::Colon,
        11 => Tok::ColonColon,
        12 => Tok::Dot,
        13 => Tok::Comma,
        14 => Tok::Semi,
        15 => Tok::Eq,
        16 => Tok::EqEq,
        17 => Tok::Ne,
        18 => Tok::Lt,
        19 => Tok::Le,
        20 => Tok::Gt,
        21 => Tok::Ge,
        22 => Tok::FatArrow,
        23 => Tok::Arrow,
        24 => Tok::LBrace,
        25 => Tok::RBrace,
        26 => Tok::LParen,
        27 => Tok::RParen,
        28 => Tok::LBracket,
        29 => Tok::RBracket,
        30 => Tok::Plus,
        31 => Tok::Minus,
        32 => Tok::Star,
        33 => Tok::Slash,
        34 => Tok::Percent,
        35 => Tok::Bang,
        36 => Tok::AndAnd,
        37 => Tok::OrOr,
        38 => Tok::Pipe,
        39 => Tok::Let,
        _ => return None,
    })
}

pub fn encode(tokens: &[Token]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    for t in tokens {
        let (kind, text) = kind_of(&t.tok);
        out.push(kind);
        out.extend_from_slice(&(t.line as u32).to_le_bytes());
        if let Some(s) = text {
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
    }
    out
}

pub fn decode(bytes: &[u8]) -> Result<Vec<Token>, String> {
    if bytes.len() < 5 || &bytes[..4] != MAGIC {
        return Err("not a compiled plugin (bad magic)".to_string());
    }
    if bytes[4] != VERSION {
        return Err(format!("unsupported compiled plugin version {}", bytes[4]));
    }
    let mut i = 5;
    let mut out = Vec::new();
    let take4 = |i: &mut usize| -> Result<u32, String> {
        let b = bytes.get(*i..*i + 4).ok_or("truncated compiled plugin")?;
        *i += 4;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    while i < bytes.len() {
        let kind = bytes[i];
        i += 1;
        let line = take4(&mut i)? as usize;
        let tok = if kind <= 4 {
            let len = take4(&mut i)? as usize;
            let raw = bytes.get(i..i + len).ok_or("truncated compiled plugin")?;
            i += len;
            let s = String::from_utf8(raw.to_vec()).map_err(|_| "corrupt compiled plugin text")?;
            match kind {
                0 => Tok::Ident(s),
                1 => Tok::Str(s),
                2 => Tok::Raw(s),
                3 => Tok::Num(s),
                _ => Tok::Hash(s),
            }
        } else {
            plain(kind).ok_or_else(|| format!("unknown token kind {}", kind))?
        };
        out.push(Token { tok, line });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex;

    #[test]
    fn round_trips_every_kind_of_token() {
        let src = "let x = 1 + 2.5 // c\nfn f(a) -> T { r#\"raw\"# \"s\\n\" #fff a::b == c != d <= e >= f && g || !h | i % j / k * l - m; [n] (o) , : . => }";
        let toks = lex(src).unwrap();
        let bytes = encode(&toks);
        assert_eq!(&bytes[..4], b"CREB");
        assert_eq!(decode(&bytes).unwrap(), toks);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(decode(b"").is_err());
        assert!(decode(b"NOPE\x01").is_err());
        assert!(decode(b"CREB\x09").unwrap_err().contains("version"));
        let mut b = encode(&lex("abc").unwrap());
        b.truncate(b.len() - 1);
        assert!(decode(&b).unwrap_err().contains("truncated"));
        let mut b = encode(&lex(";").unwrap());
        b[5] = 200;
        assert!(decode(&b).unwrap_err().contains("unknown token"));
    }

    #[test]
    fn compiled_text_is_not_plain_source_text_only_by_framing() {
        // The bytes are a token stream, not the source: whitespace and comments are gone.
        let bytes = encode(&lex("p   \"x\" // hidden comment").unwrap());
        assert!(!String::from_utf8_lossy(&bytes).contains("hidden comment"));
    }
}
