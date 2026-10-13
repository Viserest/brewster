//! The byte format a page module hands to the browser: a list of [`Element`] snapshots.
//!
//! Hand-rolled and dependency free so the SDK, the host and any runtime agree on it.
//! Integers are little endian `u32`; a string is a length followed by UTF-8 bytes.
//!
//! ```text
//! tree    := "CRET" version:u8 count:u32 element*
//! element := tag:str id:u32 nstyle:u32 (key:str nvals:u32 val:str*)* nargs:u32 arg:str*
//!            nchildren:u32 element*
//! ```

use crate::{Element, Style};

const MAGIC: &[u8; 4] = b"CRET";
const VERSION: u8 = 1;
/// A page nested deeper than this is refused instead of overflowing the stack.
const MAX_DEPTH: usize = 256;

pub fn encode(elements: &[Element]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    put_u32(&mut out, elements.len());
    for e in elements {
        put_element(&mut out, e);
    }
    out
}

pub fn decode(bytes: &[u8]) -> Result<Vec<Element>, String> {
    let mut r = Reader { bytes, pos: 0 };
    if r.take(4)? != MAGIC {
        return Err("not a page tree (bad magic)".to_string());
    }
    let v = r.take(1)?[0];
    if v != VERSION {
        return Err(format!("page tree version {} is not supported", v));
    }
    let n = r.u32()?;
    let mut out = Vec::new();
    for _ in 0..n {
        out.push(r.element(0)?);
    }
    if r.pos != bytes.len() {
        return Err("trailing bytes after the page tree".to_string());
    }
    Ok(out)
}

fn put_u32(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u32(out, s.len());
    out.extend_from_slice(s.as_bytes());
}

fn put_element(out: &mut Vec<u8>, e: &Element) {
    put_str(out, &e.tag);
    put_u32(out, e.id);
    put_u32(out, e.style.len());
    for (k, vals) in &e.style {
        put_str(out, k);
        put_u32(out, vals.len());
        for v in vals {
            put_str(out, v);
        }
    }
    put_u32(out, e.args.len());
    for a in &e.args {
        put_str(out, a);
    }
    put_u32(out, e.children.len());
    for c in &e.children {
        put_element(out, c);
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.bytes.len());
        let end = end.ok_or_else(|| "page tree is cut short".to_string())?;
        let s = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(s)
    }

    fn u32(&mut self) -> Result<usize, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    }

    fn string(&mut self) -> Result<String, String> {
        let n = self.u32()?;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| "page tree holds invalid UTF-8".to_string())
    }

    fn element(&mut self, depth: usize) -> Result<Element, String> {
        if depth > MAX_DEPTH {
            return Err("page tree is nested too deeply".to_string());
        }
        let tag = self.string()?;
        let id = self.u32()?;
        let mut style = Style::new();
        for _ in 0..self.u32()? {
            let key = self.string()?;
            let mut vals = Vec::new();
            for _ in 0..self.u32()? {
                vals.push(self.string()?);
            }
            style.insert(key, vals);
        }
        let mut args = Vec::new();
        for _ in 0..self.u32()? {
            args.push(self.string()?);
        }
        let mut children = Vec::new();
        for _ in 0..self.u32()? {
            children.push(self.element(depth + 1)?);
        }
        Ok(Element {
            tag,
            style,
            args,
            children,
            id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Element> {
        let mut style = Style::new();
        style.insert("pad".into(), vec!["1".into(), "2".into()]);
        vec![Element {
            tag: "box".into(),
            style,
            args: vec![],
            id: 7,
            children: vec![Element {
                tag: "p".into(),
                style: Style::new(),
                args: vec!["héllo".into()],
                children: vec![],
                id: 8,
            }],
        }]
    }

    #[test]
    fn round_trips() {
        let s = sample();
        assert_eq!(decode(&encode(&s)).unwrap(), s);
        assert_eq!(decode(&encode(&[])).unwrap(), vec![]);
    }

    #[test]
    fn refuses_bad_input() {
        let good = encode(&sample());
        assert!(decode(b"nope").is_err());
        assert!(decode(&good[..good.len() - 1]).is_err());
        let mut extra = good.clone();
        extra.push(0);
        assert!(decode(&extra).is_err());
        let mut v2 = good;
        v2[4] = 2;
        assert!(decode(&v2).is_err());
    }

    #[test]
    fn huge_counts_do_not_allocate_or_loop_forever() {
        let mut b = encode(&[]);
        b[5..9].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(&b).is_err());
    }

    #[test]
    fn deep_nesting_is_refused() {
        let mut e = Element::default();
        e.tag = "box".into();
        for _ in 0..MAX_DEPTH + 5 {
            let mut p = Element::default();
            p.tag = "box".into();
            p.children.push(e);
            e = p;
        }
        assert!(decode(&encode(&[e])).is_err());
    }
}
