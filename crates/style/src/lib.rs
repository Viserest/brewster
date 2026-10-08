//! Style value parsing: colors, spacing, alignment, text entities.
//! Units are terminal cells for now.

use std::collections::BTreeMap;

pub type StyleMap = BTreeMap<String, Vec<String>>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Start,
    Center,
    End,
}

fn bad_color(s: &str) -> String {
    format!("unknown color `{}`", s)
}

pub fn parse_color(s: &str) -> Result<Rgb, String> {
    match s {
        "black" => Ok(Rgb(0, 0, 0)),
        "white" => Ok(Rgb(255, 255, 255)),
        "red" => Ok(Rgb(220, 50, 47)),
        "green" => Ok(Rgb(40, 160, 60)),
        "blue" => Ok(Rgb(60, 120, 240)),
        "yellow" => Ok(Rgb(230, 200, 40)),
        "cyan" => Ok(Rgb(40, 190, 200)),
        "magenta" => Ok(Rgb(200, 60, 180)),
        "gray" | "grey" => Ok(Rgb(128, 128, 128)),
        _ => {
            let h = s.strip_prefix('#').ok_or_else(|| bad_color(s))?;
            if !h.is_ascii() {
                return Err(bad_color(s));
            }
            let p = |t: &str| u8::from_str_radix(t, 16).map_err(|_| bad_color(s));
            match h.len() {
                6 => Ok(Rgb(p(&h[0..2])?, p(&h[2..4])?, p(&h[4..6])?)),
                3 => Ok(Rgb(p(&h[0..1])? * 17, p(&h[1..2])? * 17, p(&h[2..3])? * 17)),
                _ => Err(bad_color(s)),
            }
        }
    }
}

pub fn parse_align(s: &str) -> Result<Align, String> {
    match s {
        "middle" | "center" => Ok(Align::Center),
        "start" | "left" => Ok(Align::Start),
        "end" | "right" => Ok(Align::End),
        o => Err(format!(
            "unknown alignment `{}` (use start, middle or end)",
            o
        )),
    }
}

pub fn first(v: &[String]) -> Result<&str, String> {
    v.first()
        .map(|s| s.as_str())
        .ok_or_else(|| "missing value".to_string())
}

fn num(s: &str) -> Result<usize, String> {
    s.parse::<usize>().map_err(|_| {
        format!(
            "`{}` is not a non-negative whole number (units are terminal cells)",
            s
        )
    })
}

/// Returns `[top, right, bottom, left]` for `key` (e.g. "pad" or "margin").
/// Shorthand: 1 value = all sides, 2 values = x,y, 4 values = top,right,bottom,left.
/// `key-x` and `key-y` override the horizontal / vertical sides.
pub fn spacing(style: &StyleMap, key: &str) -> Result<[usize; 4], String> {
    let mut sp = [0usize; 4];
    if let Some(v) = style.get(key) {
        match v.len() {
            1 => sp = [num(&v[0])?; 4],
            2 => {
                let x = num(&v[0])?;
                let y = num(&v[1])?;
                sp = [y, x, y, x];
            }
            4 => {
                for i in 0..4 {
                    sp[i] = num(&v[i])?;
                }
            }
            _ => {
                return Err(format!(
                    "`{}` takes 1, 2 (x,y) or 4 (top,right,bottom,left) values",
                    key
                ));
            }
        }
    }
    if let Some(v) = style.get(&format!("{}-x", key)) {
        let x = num(first(v)?)?;
        sp[1] = x;
        sp[3] = x;
    }
    if let Some(v) = style.get(&format!("{}-y", key)) {
        let y = num(first(v)?)?;
        sp[0] = y;
        sp[2] = y;
    }
    Ok(sp)
}

/// Border widths as `[top, right, bottom, left]`, same shorthand as `pad` / `margin`.
/// Terminal borders are exactly 0 or 1 cell thick.
pub fn border(style: &StyleMap) -> Result<[usize; 4], String> {
    let b = spacing(style, "border")?;
    if b.iter().any(|&n| n > 1) {
        return Err(
            "`border` values must be 0 or 1 (a terminal border is one cell thick)".to_string(),
        );
    }
    Ok(b)
}

pub fn decode(s: &str) -> String {
    s.replace("&copy;", "©")
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(parse_color("#fff"), Ok(Rgb(255, 255, 255)));
        assert_eq!(parse_color("#181818"), Ok(Rgb(0x18, 0x18, 0x18)));
        assert!(parse_color("nope").is_err());
    }

    #[test]
    fn border_is_zero_or_one() {
        let mut m = StyleMap::new();
        m.insert("border".to_string(), vec!["1".to_string(), "0".to_string()]);
        assert_eq!(border(&m), Ok([0, 1, 0, 1]));
        m.insert("border".to_string(), vec!["2".to_string()]);
        assert!(border(&m).is_err());
    }

    #[test]
    fn spacing_shorthand() {
        let mut m = StyleMap::new();
        m.insert("pad".to_string(), vec!["1".to_string(), "2".to_string()]);
        assert_eq!(spacing(&m, "pad"), Ok([2, 1, 2, 1]));
        m.insert("pad-x".to_string(), vec!["5".to_string()]);
        assert_eq!(spacing(&m, "pad"), Ok([2, 5, 2, 5]));
    }
}
