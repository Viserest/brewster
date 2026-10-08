//! Layout: turns a resolved tree into rows of styled text spans (cell units).
//! Output is renderer-agnostic; apps decide how to paint it.

use resolver::Resolved;
use style::{Align, border, decode, first, parse_align, parse_color, spacing};

pub use style::Rgb;

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub underline: bool,
}

/// One line of output. The visible width of every row equals the requested width.
pub type Row = Vec<Span>;

/// Concatenated text of a row, without any styling.
pub fn row_text(row: &Row) -> String {
    row.iter().map(|s| s.text.as_str()).collect()
}

#[derive(Clone, Copy)]
struct Ctx {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    ax: Align,
}

/// Lays out `root` into rows that are `width` cells wide.
pub fn layout(root: &Resolved, width: usize) -> Result<Vec<Row>, String> {
    let ctx = Ctx {
        fg: None,
        bg: None,
        ax: Align::Start,
    };
    layout_node(root, width, ctx)
}

fn wrap(text: &str, w: usize) -> Vec<String> {
    let w = w.max(1);
    let mut lines: Vec<String> = Vec::new();
    for para in text.split('\n') {
        let mut cur = String::new();
        let mut n = 0usize;
        for word in para.split_whitespace() {
            let mut word: Vec<char> = word.chars().collect();
            while word.len() > w {
                if n > 0 {
                    lines.push(std::mem::take(&mut cur));
                    n = 0;
                }
                lines.push(word.drain(..w).collect());
            }
            let wl = word.len();
            if wl == 0 {
                continue;
            }
            if n == 0 {
                cur = word.iter().collect();
                n = wl;
            } else if n + 1 + wl <= w {
                cur.push(' ');
                cur.extend(word.iter());
                n += 1 + wl;
            } else {
                lines.push(std::mem::take(&mut cur));
                cur = word.iter().collect();
                n = wl;
            }
        }
        lines.push(cur);
    }
    lines
}

fn align_line(s: &str, w: usize, a: Align) -> String {
    let n = s.chars().count();
    let gap = w.saturating_sub(n);
    let (l, r) = match a {
        Align::Start => (0, gap),
        Align::Center => (gap / 2, gap - gap / 2),
        Align::End => (gap, 0),
    };
    format!("{}{}{}", " ".repeat(l), s, " ".repeat(r))
}

struct Leaf {
    text: String,
    bold: bool,
    underline: bool,
    default_fg: Option<Rgb>,
    verbatim: bool,
}

fn leaf(r: &Resolved) -> Result<Leaf, String> {
    let a0 = r.args.first().cloned().unwrap_or_default();
    let plain = |text: String, bold: bool| Leaf {
        text,
        bold,
        underline: false,
        default_fg: None,
        verbatim: false,
    };
    match r.tag.as_str() {
        "h1" => Ok(plain(decode(&a0).to_uppercase(), true)),
        "h2" | "h3" => Ok(plain(decode(&a0), true)),
        "h4" | "h5" | "h6" | "p" | "span" => Ok(plain(decode(&a0), false)),
        "link" => {
            let text = r.args.get(1).unwrap_or(&a0);
            Ok(Leaf {
                text: decode(text),
                bold: false,
                underline: true,
                default_fg: Some(Rgb(80, 170, 255)),
                verbatim: false,
            })
        }
        "button" => {
            let label = if a0.is_empty() {
                "Button".to_string()
            } else {
                decode(&a0)
            };
            Ok(plain(format!("[ {} ]", label), true))
        }
        "input" => {
            let val = r.args.get(1).cloned().unwrap_or_default();
            let shown = if r.style.contains_key("protected") {
                "*".repeat(val.chars().count())
            } else {
                decode(&val)
            };
            Ok(plain(format!("{}: [{}]", a0, shown), false))
        }
        "pre" => {
            let s = a0.strip_prefix('\n').unwrap_or(a0.as_str());
            let s = s.strip_suffix('\n').unwrap_or(s);
            Ok(Leaf {
                text: s.to_string(),
                bold: false,
                underline: false,
                default_fg: None,
                verbatim: true,
            })
        }
        t => Err(format!("cannot lay out element `{}`", t)),
    }
}

fn layout_node(r: &Resolved, width: usize, ctx: Ctx) -> Result<Vec<Row>, String> {
    let st = &r.style;
    let [mt, mr, mb, ml] = spacing(st, "margin")?;
    let [pt, pr, pb, pl] = spacing(st, "pad")?;
    let [bt, br, bb, bl] = border(st)?;

    let fg = match st.get("fcolor") {
        Some(v) => Some(parse_color(first(v)?)?),
        None => ctx.fg,
    };
    let ibg = match st.get("bcolor") {
        Some(v) => Some(parse_color(first(v)?)?),
        None => ctx.bg,
    };
    let ax = match st.get("align-x").or_else(|| st.get("align")) {
        Some(v) => parse_align(first(v)?)?,
        None => ctx.ax,
    };

    let inner_w = width.saturating_sub(ml + mr + bl + br + pl + pr).max(1);
    let mut inner: Vec<Row> = Vec::new();

    if r.tag == "box" {
        let child_ctx = Ctx { fg, bg: ibg, ax };
        for c in &r.children {
            inner.extend(layout_node(c, inner_w, child_ctx)?);
        }
    } else {
        let lf = leaf(r)?;
        let tfg = if st.contains_key("fcolor") {
            fg
        } else {
            lf.default_fg.or(ctx.fg)
        };
        let (lines, a): (Vec<String>, Align) = if lf.verbatim {
            (
                lf.text
                    .lines()
                    .map(|l| l.chars().take(inner_w).collect::<String>())
                    .collect(),
                Align::Start,
            )
        } else {
            (wrap(&lf.text, inner_w), ax)
        };
        for l in &lines {
            inner.push(vec![Span {
                text: align_line(l, inner_w, a),
                fg: tfg,
                bg: ibg,
                bold: lf.bold,
                underline: lf.underline,
            }]);
        }
    }

    let border_fg = match st.get("border-color") {
        Some(v) => Some(parse_color(first(v)?)?),
        None => fg,
    };
    let space = |n: usize, bg: Option<Rgb>| Span {
        text: " ".repeat(n),
        fg: None,
        bg,
        bold: false,
        underline: false,
    };
    let edge = |text: String| Span {
        text,
        fg: border_fg,
        bg: ibg,
        bold: false,
        underline: false,
    };
    // Layers, outside in: margin, border, padding, content.
    let row = |mid: &Row| -> Row {
        let mut line: Row = Vec::new();
        line.push(space(ml, ctx.bg));
        if bl > 0 {
            line.push(edge("\u{2502}".to_string()));
        }
        line.push(space(pl, ibg));
        line.extend(mid.iter().cloned());
        line.push(space(pr, ibg));
        if br > 0 {
            line.push(edge("\u{2502}".to_string()));
        }
        line.push(space(mr, ctx.bg));
        line
    };
    // Top or bottom border line, with corners only where a side border meets it.
    let horizontal = |left: &str, right: &str| -> Row {
        let mut text = String::new();
        if bl > 0 {
            text.push_str(left);
        }
        text.push_str(&"\u{2500}".repeat(pl + inner_w + pr));
        if br > 0 {
            text.push_str(right);
        }
        vec![space(ml, ctx.bg), edge(text), space(mr, ctx.bg)]
    };
    let full: Row = vec![space(width, ctx.bg)];
    let pad_mid: Row = vec![space(inner_w, ibg)];

    let mut out: Vec<Row> = Vec::new();
    for _ in 0..mt {
        out.push(full.clone());
    }
    if bt > 0 {
        out.push(horizontal("\u{250C}", "\u{2510}"));
    }
    for _ in 0..pt {
        out.push(row(&pad_mid));
    }
    for l in &inner {
        out.push(row(l));
    }
    for _ in 0..pb {
        out.push(row(&pad_mid));
    }
    if bb > 0 {
        out.push(horizontal("\u{2514}", "\u{2518}"));
    }
    for _ in 0..mb {
        out.push(full.clone());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_words() {
        assert_eq!(wrap("a b c", 3), vec!["a b".to_string(), "c".to_string()]);
    }

    #[test]
    fn full_border_surrounds_content() {
        use resolver::Style;
        let mut style = Style::new();
        style.insert("border".to_string(), vec!["1".to_string()]);
        let node = Resolved {
            tag: "p".to_string(),
            style,
            args: vec!["hi".to_string()],
            children: vec![],
        };
        let rows = layout(&node, 6).unwrap();
        let text: Vec<String> = rows.iter().map(row_text).collect();
        assert_eq!(
            text,
            vec![
                "\u{250C}\u{2500}\u{2500}\u{2500}\u{2500}\u{2510}".to_string(),
                "\u{2502}hi  \u{2502}".to_string(),
                "\u{2514}\u{2500}\u{2500}\u{2500}\u{2500}\u{2518}".to_string(),
            ]
        );
    }

    #[test]
    fn x_only_border_has_no_top_or_bottom() {
        use resolver::Style;
        let mut style = Style::new();
        style.insert("border-x".to_string(), vec!["1".to_string()]);
        let node = Resolved {
            tag: "p".to_string(),
            style,
            args: vec!["a".to_string()],
            children: vec![],
        };
        let rows = layout(&node, 4).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(row_text(&rows[0]), "\u{2502}a \u{2502}");
    }

    #[test]
    fn centers_text() {
        assert_eq!(align_line("ab", 6, Align::Center), "  ab  ");
    }
}
