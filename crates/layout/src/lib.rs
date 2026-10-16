//! Layout: turns a document tree into rows of styled text spans (cell units).
//! Output is renderer-agnostic; apps decide how to paint it.
//!
//! Every element is a box, outside in: margin, border, pad, content. `size` is the
//! box including border and pad but excluding margin. The default is `min,min`, so an
//! element's background, underline, border, ... end right after its content; `size:max`
//! fills the parent instead. `align` / `align-x` place an element inside its parent's
//! content area (children inherit it) and align the text inside a wider text element.

use std::collections::HashMap;

use style::{
    Align, Dim, Size, border, border_fading, border_radius, decode, first, parse_align,
    parse_color, size, spacing,
};
use tree::Element;

pub use style::{Fade, Rgb};

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub underline: bool,
    /// Font size multiplier (headers). A scaled span covers `chars * scale` cells and
    /// `scale` rows: the row it is on plus `scale - 1` continuation rows. Only produced
    /// when [`LayoutOptions::scale_headers`] is set; terminals cannot draw it.
    pub scale: u8,
}

impl Span {
    /// Width in cells.
    pub fn cells(&self) -> usize {
        self.text.chars().count() * self.scale.max(1) as usize
    }
}

/// What kind of element a row's text belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Link,
    Button,
    Input,
    Pre,
}

/// Metadata for a row that shows element text (rows of pure margin, border or
/// padding have none).
#[derive(Debug, Clone, PartialEq)]
pub struct LineInfo {
    pub kind: Kind,
    /// What "copy" yields for this row: the visual line for text and `pre`, the whole
    /// text for a link, the label for a button, the current value for an input.
    pub text: String,
    /// Cells `[start, end)` of this row that hold the element's text.
    pub cols: (usize, usize),
    /// Shared by every row of one link / button / input.
    pub focus: Option<usize>,
    /// Index of the input in document order (also a focus target).
    pub input: Option<usize>,
    /// Column of the text caret when this input is being edited and the caret is on this row.
    pub caret: Option<usize>,
    /// Where a link points (entities decoded).
    pub href: Option<String>,
    /// Id of the tree node this row belongs to (`tree::Element::id`).
    pub node: usize,
}

/// One line of output. Rows are as wide as the requested width.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Row {
    pub spans: Vec<Span>,
    pub info: Option<LineInfo>,
    /// A continuation of the scaled text on the row above: it only carries background.
    pub cont: bool,
}

impl Row {
    /// Visible width in cells.
    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.cells()).sum()
    }

    fn shifted(mut self, dx: usize) -> Row {
        if let Some(i) = &mut self.info {
            i.cols.0 += dx;
            i.cols.1 += dx;
            if let Some(c) = &mut i.caret {
                *c += dx;
            }
        }
        self
    }
}

/// Concatenated text of a row, without any styling.
pub fn row_text(row: &Row) -> String {
    row.spans.iter().map(|s| s.text.as_str()).collect()
}

/// Runtime state of the page's inputs, keyed by input index (document order).
#[derive(Debug, Clone, Default)]
pub struct Form {
    /// Edited values. Inputs without an entry show the value written in the source.
    pub values: HashMap<usize, String>,
    /// `(input, caret)` while an input is being edited; the caret is a char index into its value.
    pub editing: Option<(usize, usize)>,
}

/// How one input was laid out. The displayed text is `prefix` chars (`name: [`), the value
/// (or `*`s when protected) and a closing `]`; `segs` are the visual lines as char ranges
/// into that displayed text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputLayout {
    pub value: String,
    pub prefix: usize,
    pub segs: Vec<(usize, usize)>,
    pub protected: bool,
    /// Id of the input's tree node.
    pub node: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Page {
    pub rows: Vec<Row>,
    /// Indexed by input index.
    pub inputs: Vec<InputLayout>,
    /// Shapes to draw behind the text, in page coordinates, back to front. Only produced
    /// when [`LayoutOptions::graphical`] is set.
    pub decos: Vec<Deco>,
}

/// What a [`Deco`] is for; frontends may style kinds differently (a focus ring, say).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoKind {
    /// A box with a border or a rounded background.
    Box,
    Button,
    Link,
    Input,
}

/// A rectangle drawn behind the text of a page by a graphical frontend: a rounded
/// background, a border, or both. Units are cells; `inset` moves the edges in by a part of
/// a cell, and the radius is in points.
#[derive(Debug, Clone, PartialEq)]
pub struct Deco {
    pub col: usize,
    pub row: usize,
    pub cols: usize,
    pub rows: usize,
    /// Fractions of a cell the left/right and top/bottom edges are moved in.
    pub inset: (f32, f32),
    pub fill: Option<Rgb>,
    pub stroke: Option<Rgb>,
    /// Which sides of the stroke are drawn: top, right, bottom, left.
    pub sides: [bool; 4],
    pub radius: f32,
    pub fade: Fade,
    pub kind: DecoKind,
    /// The focus id of the link / button / input this belongs to (`LineInfo::focus`).
    pub focus: Option<usize>,
}

/// How buttons, links and inputs look in a graphical frontend: they are similar rounded
/// shapes, each with its own fill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlStyle {
    pub fg: Option<Rgb>,
    pub fill: Option<Rgb>,
    pub stroke: Option<Rgb>,
    /// Corner radius in points ([`style::FULL_RADIUS`] for a pill).
    pub radius: f32,
    pub fade: Fade,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Controls {
    pub button: ControlStyle,
    pub link: ControlStyle,
    pub input: ControlStyle,
    /// The color of an input's name while it is empty.
    pub placeholder: Rgb,
}

impl Controls {
    fn of(&self, kind: DecoKind) -> ControlStyle {
        match kind {
            DecoKind::Button => self.button,
            DecoKind::Link => self.link,
            _ => self.input,
        }
    }
}

/// How a page is fitted to the screen.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutOptions {
    /// Height of the screen in rows. The document is `max` wide and high, so a top-level
    /// element with `size:max` (or `max` height) fills the rows left on the screen; it
    /// never gets shorter than its content, so long pages still scroll.
    pub viewport_rows: Option<usize>,
    /// Draw header font sizes by scaling (GUI). Without it headers differ in boldness,
    /// underline and case only.
    pub scale_headers: bool,
    /// Draw for a window instead of a terminal: borders and rounded backgrounds become
    /// [`Deco`] shapes (the border cells stay as gutters), and buttons, links and inputs
    /// become padded shapes with the given look, without brackets.
    pub graphical: Option<Controls>,
}

/// Lays out one element into rows that are `width` cells wide.
pub fn layout(root: &Element, width: usize) -> Result<Vec<Row>, String> {
    Ok(layout_all(std::slice::from_ref(root), width, &Form::default())?.rows)
}

/// Lays out top-level elements in order. Focus and input numbering run across all of them.
pub fn layout_all(roots: &[Element], width: usize, form: &Form) -> Result<Page, String> {
    layout_all_with(roots, width, form, LayoutOptions::default())
}

pub fn layout_all_with(
    roots: &[Element],
    width: usize,
    form: &Form,
    opts: LayoutOptions,
) -> Result<Page, String> {
    let width = width.max(1);
    let ctx = Ctx {
        fg: None,
        bg: None,
        ax: Align::Start,
    };
    let mut ids = Ids::default();
    let mut items: Vec<Item> = Vec::new();
    for r in roots {
        items.push(build(r, ctx, &mut ids, form, opts)?);
    }
    let mut st = State {
        inputs: vec![InputLayout::default(); ids.input],
    };
    let mut rows: Vec<Row> = Vec::new();
    let mut decos: Vec<Deco> = Vec::new();
    for it in &items {
        let remaining = opts.viewport_rows.map(|v| v.saturating_sub(rows.len()));
        let mut blk = place(it, width, None, &mut st);
        if let (Some(rem), Dim::Max) = (remaining, it.size.h) {
            // Fill the rest of the screen, but never cut the content short.
            if blk.rows.len() < rem {
                blk = place(it, width, Some(rem), &mut st);
            }
        }
        let gap = width.saturating_sub(blk.w);
        let left = offset(it.ax, gap);
        decos.extend(shifted(blk.decos, left, rows.len()));
        for r in blk.rows {
            rows.push(pad_row(r, left, gap - left, None));
        }
    }
    Ok(Page {
        rows,
        inputs: st.inputs,
        decos,
    })
}

// ───────────────────────── Build: tree -> items ─────────────────────────

#[derive(Clone, Copy)]
struct Ctx {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    ax: Align,
}

#[derive(Default)]
struct Ids {
    focus: usize,
    input: usize,
}

impl Ids {
    fn next_focus(&mut self) -> usize {
        let i = self.focus;
        self.focus += 1;
        i
    }
}

struct State {
    inputs: Vec<InputLayout>,
}

/// A styled element with everything layout needs, computed once.
struct Item {
    margin: [usize; 4],
    pad: [usize; 4],
    border: [usize; 4],
    size: Size,
    /// Background of the parent: shows in margins and in gaps left by alignment.
    outer_bg: Option<Rgb>,
    bg: Option<Rgb>,
    border_fg: Option<Rgb>,
    ax: Align,
    /// Natural content width: the widest the content is when nothing wraps.
    nat: usize,
    body: Body,
    /// The background of this element's own cells (border, padding, text). It is the
    /// parent's where a shape fills the box instead.
    frame_bg: Option<Rgb>,
    /// Rounded corners (a terminal draws `╭╮╰╯`).
    round: bool,
    gfx: Option<Gfx>,
}

/// The shape a graphical frontend draws for an element.
struct Gfx {
    kind: DecoKind,
    fill: Option<Rgb>,
    stroke: Option<Rgb>,
    sides: [bool; 4],
    /// The border cells are gutters the stroke runs through (as opposed to a stroke on the
    /// edge of the shape).
    gutter: bool,
    radius: f32,
    fade: Fade,
    focus: Option<usize>,
}

impl Gfx {
    fn deco(&self, col: usize, row: usize, cols: usize, rows: usize) -> Deco {
        let (ix, iy) = if self.gutter {
            // Run the stroke through the middle of the gutter, on the sides that have one.
            (0.5, 0.5)
        } else if self.kind == DecoKind::Box {
            (0.0, 0.0)
        } else {
            // Stacked controls do not touch.
            (0.0, 0.1)
        };
        Deco {
            col,
            row,
            cols,
            rows,
            inset: (ix, iy),
            fill: self.fill,
            stroke: self.stroke,
            sides: self.sides,
            radius: self.radius,
            fade: self.fade,
            kind: self.kind,
            focus: self.focus,
        }
    }
}

fn shifted(mut decos: Vec<Deco>, dx: usize, dy: usize) -> Vec<Deco> {
    for d in &mut decos {
        d.col += dx;
        d.row += dy;
    }
    decos
}

enum Body {
    Box(Vec<Item>),
    Text(TextBody),
    Input(InputBody),
}

struct TextBody {
    kind: Kind,
    text: String,
    /// Copy text for the whole element (link text, button label).
    copy: Option<String>,
    fg: Option<Rgb>,
    bold: bool,
    underline: bool,
    /// `pre`: no wrapping, no whitespace collapsing, alignment fixed to the start.
    verbatim: bool,
    /// Font size multiplier; 1 unless headers are scaled.
    scale: usize,
    focus: Option<usize>,
    href: Option<String>,
    node: usize,
}

struct InputBody {
    id: usize,
    node: usize,
    focus: usize,
    name: String,
    protected: bool,
    value: String,
    /// Caret as a char index into `value`, while this input is being edited.
    caret: Option<usize>,
    fg: Option<Rgb>,
    /// Drawn for a window: no `name: [ ]`, the name shows while the value is empty.
    graphical: Option<Rgb>,
}

/// Per-row attributes shared by all rows of a leaf.
struct Look {
    kind: Kind,
    fg: Option<Rgb>,
    bold: bool,
    underline: bool,
    verbatim: bool,
    scale: usize,
    focus: Option<usize>,
    input: Option<usize>,
    href: Option<String>,
    node: usize,
}

impl TextBody {
    fn new(kind: Kind, text: String, fg: Option<Rgb>, node: usize) -> TextBody {
        TextBody {
            kind,
            text,
            copy: None,
            fg,
            bold: false,
            underline: false,
            verbatim: false,
            scale: 1,
            focus: None,
            href: None,
            node,
        }
    }

    fn look(&self) -> Look {
        Look {
            kind: self.kind,
            fg: self.fg,
            bold: self.bold,
            underline: self.underline,
            verbatim: self.verbatim,
            scale: 1,
            focus: self.focus,
            input: None,
            href: self.href.clone(),
            node: self.node,
        }
    }

    fn natural(&self) -> usize {
        if self.verbatim {
            return self
                .text
                .lines()
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0);
        }
        let widest = self
            .text
            .split('\n')
            .map(|p| {
                let words: Vec<&str> = p.split_whitespace().collect();
                words.iter().map(|w| w.chars().count()).sum::<usize>()
                    + words.len().saturating_sub(1)
            })
            .max()
            .unwrap_or(0);
        widest * self.scale
    }
}

impl InputBody {
    /// Displayed chars and the length of the `name: [` prefix. The closing `]` is last.
    fn displayed(&self) -> (Vec<char>, usize) {
        if self.graphical.is_some() {
            // Just the value, and a blank cell for the caret to rest on after it.
            let mark = if self.protected { '\u{2022}' } else { ' ' };
            let mut d: Vec<char> = if self.protected {
                std::iter::repeat(mark).take(self.value.chars().count()).collect()
            } else {
                self.value.chars().collect()
            };
            d.push(' ');
            return (d, 0);
        }
        let mut d: Vec<char> = format!("{}: [", self.name).chars().collect();
        let prefix = d.len();
        if self.protected {
            d.extend(std::iter::repeat('*').take(self.value.chars().count()));
        } else {
            d.extend(self.value.chars());
        }
        d.push(']');
        (d, prefix)
    }

    /// The name stands in for an empty value that is not being edited.
    fn placeholder(&self) -> bool {
        self.graphical.is_some() && self.value.is_empty() && self.caret.is_none()
    }

    fn look(&self) -> Look {
        Look {
            kind: Kind::Input,
            fg: if self.placeholder() { self.graphical } else { self.fg },
            bold: false,
            underline: false,
            verbatim: false,
            scale: 1,
            focus: Some(self.focus),
            input: Some(self.id),
            href: None,
            node: self.node,
        }
    }
}

impl Item {
    fn chrome_h(&self) -> usize {
        self.pad[1] + self.pad[3] + self.border[1] + self.border[3]
    }

    /// Natural width including border, pad and margin: what a parent needs to hold it.
    fn outer_nat(&self) -> usize {
        let m = self.margin[1] + self.margin[3];
        m + match self.size.w {
            Dim::Cells(n) => n,
            _ => self.nat + self.chrome_h(),
        }
    }
}

/// `h1`..`h6`: (uppercase, bold, underline, font size). Headers differ in boldness,
/// underline and size (size only where the renderer can scale); the upper case of the
/// big ones keeps them apart in a terminal too.
fn heading_style(tag: &str) -> (bool, bool, bool, usize) {
    match tag {
        "h1" => (true, true, false, 3),
        "h2" => (true, true, false, 2),
        "h3" => (true, false, false, 2),
        "h4" => (false, true, false, 1),
        "h5" => (false, true, true, 1),
        _ => (false, false, true, 1),
    }
}

fn build(
    r: &Element,
    ctx: Ctx,
    ids: &mut Ids,
    form: &Form,
    opts: LayoutOptions,
) -> Result<Item, String> {
    let st = &r.style;
    let margin = spacing(st, "margin")?;
    let pad = spacing(st, "pad")?;
    let bord = border(st)?;
    let sz = size(st)?;
    let radius = border_radius(st)?;
    let fade = border_fading(st)?;
    let control = match (opts.graphical, r.tag.as_str()) {
        (Some(c), "button") => Some((DecoKind::Button, c.of(DecoKind::Button))),
        (Some(c), "link") => Some((DecoKind::Link, c.of(DecoKind::Link))),
        (Some(c), "input") => Some((DecoKind::Input, c.of(DecoKind::Input))),
        _ => None,
    };
    let mut pad = pad;
    if control.is_some()
        && !["pad", "pad-x", "pad-y"].iter().any(|k| st.contains_key(*k))
    {
        pad = [0, 1, 0, 1];
    }

    let fg = match st.get("fcolor") {
        Some(v) => Some(parse_color(first(v)?)?),
        None => ctx.fg,
    };
    let bg = match st.get("bcolor") {
        Some(v) => Some(parse_color(first(v)?)?),
        None => ctx.bg,
    };
    let ax = match st.get("align-x").or_else(|| st.get("align")) {
        Some(v) => parse_align(first(v)?)?,
        None => ctx.ax,
    };
    let border_fg = match st.get("border-color") {
        Some(v) => Some(parse_color(first(v)?)?),
        None => fg,
    };
    let explicit_bg = st.contains_key("bcolor");
    let has_border = bord.iter().any(|&b| b > 0);
    let r_pts = radius.or(control.map(|(_, c)| c.radius)).unwrap_or(0.0);

    // The shape a window draws for this element.
    let gfx = if opts.graphical.is_some() {
        match control {
            Some((kind, c)) => {
                let fill = if explicit_bg { bg } else { c.fill };
                let (stroke, sides, gutter) = if has_border {
                    (border_fg, bord.map(|b| b > 0), true)
                } else {
                    let stroke = match st.get("border-color") {
                        Some(_) => border_fg,
                        None => c.stroke,
                    };
                    (stroke, [true; 4], false)
                };
                Some(Gfx {
                    kind,
                    fill,
                    stroke,
                    sides,
                    gutter,
                    radius: r_pts,
                    fade: if fade == Fade::None { c.fade } else { fade },
                    focus: None,
                })
            }
            None if has_border || (explicit_bg && r_pts > 0.0) => Some(Gfx {
                kind: DecoKind::Box,
                fill: (explicit_bg && r_pts > 0.0).then_some(bg).flatten(),
                stroke: has_border.then_some(border_fg).flatten(),
                sides: bord.map(|b| b > 0),
                gutter: has_border,
                radius: r_pts,
                fade,
                focus: None,
            }),
            None => None,
        }
    } else {
        None
    };
    // Where a shape fills the box, the element's own cells keep the parent's background.
    let cell_bg = if gfx.as_ref().is_some_and(|g| g.fill.is_some()) {
        ctx.bg
    } else {
        bg
    };

    let (body, nat) = if r.tag == "box" {
        let child_ctx = Ctx { fg, bg, ax };
        let mut kids: Vec<Item> = Vec::new();
        let mut nat = 0usize;
        for c in &r.children {
            let k = build(c, child_ctx, ids, form, opts)?;
            nat = nat.max(k.outer_nat());
            kids.push(k);
        }
        (Body::Box(kids), nat)
    } else {
        build_leaf(r, fg, ctx, ids, form, opts, control.map(|(_, c)| c))?
    };

    let mut gfx = gfx;
    if let Some(g) = &mut gfx {
        g.focus = match &body {
            Body::Text(t) => t.focus,
            Body::Input(b) => Some(b.focus),
            Body::Box(_) => None,
        };
    }
    // A leaf's text sits on its own cells; a box's children sit on the box's background.
    let is_box = matches!(body, Body::Box(_));
    Ok(Item {
        margin,
        pad,
        border: bord,
        size: sz,
        outer_bg: ctx.bg,
        bg: if is_box { bg } else { cell_bg },
        border_fg,
        ax,
        nat,
        body,
        frame_bg: cell_bg,
        round: r_pts > 0.0,
        gfx,
    })
}

fn build_leaf(
    r: &Element,
    fg: Option<Rgb>,
    ctx: Ctx,
    ids: &mut Ids,
    form: &Form,
    opts: LayoutOptions,
    control: Option<ControlStyle>,
) -> Result<(Body, usize), String> {
    let st = &r.style;
    let a0 = r.args.first().cloned().unwrap_or_default();
    // An explicit `fcolor` wins; otherwise the element's own default, then the inherited color.
    let text_fg = |default: Option<Rgb>| {
        if st.contains_key("fcolor") {
            fg
        } else {
            default.or(ctx.fg)
        }
    };
    // A control in a window has its look's text color.
    let control_fg = control.and_then(|c| c.fg);

    let text = |t: TextBody| {
        let nat = t.natural();
        Ok((Body::Text(t), nat))
    };

    match r.tag.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let (upper, bold, underline, size) = heading_style(&r.tag);
            let mut s = decode(&a0);
            if upper {
                s = s.to_uppercase();
            }
            let mut t = TextBody::new(Kind::Text, s, text_fg(None), r.id);
            t.bold = bold;
            t.underline = underline;
            if opts.scale_headers {
                t.scale = size;
            }
            text(t)
        }
        "p" | "span" => text(TextBody::new(Kind::Text, decode(&a0), text_fg(None), r.id)),
        "link" => {
            let label = decode(r.args.get(1).unwrap_or(&a0));
            let mut t = TextBody::new(
                Kind::Link,
                label.clone(),
                text_fg(control_fg.or(Some(Rgb(80, 170, 255)))),
                r.id,
            );
            t.underline = control.is_none();
            t.copy = Some(label);
            t.href = Some(decode(&a0));
            t.focus = Some(ids.next_focus());
            text(t)
        }
        "button" => {
            let label = if a0.is_empty() {
                "Button".to_string()
            } else {
                decode(&a0)
            };
            let shown = if control.is_some() {
                label.clone()
            } else {
                format!("[ {} ]", label)
            };
            let mut t = TextBody::new(Kind::Button, shown, text_fg(control_fg), r.id);
            t.bold = true;
            t.copy = Some(label);
            t.focus = Some(ids.next_focus());
            text(t)
        }
        "pre" => {
            let s = a0.strip_prefix('\n').unwrap_or(a0.as_str());
            let s = s.strip_suffix('\n').unwrap_or(s);
            let mut t = TextBody::new(Kind::Pre, s.to_string(), text_fg(None), r.id);
            t.verbatim = true;
            text(t)
        }
        "input" => {
            let id = ids.input;
            ids.input += 1;
            let focus = ids.next_focus();
            let value = match form.values.get(&id) {
                Some(v) => v.clone(),
                None => decode(&r.args.get(1).cloned().unwrap_or_default()),
            };
            let caret = match form.editing {
                Some((e, c)) if e == id => Some(c),
                _ => None,
            };
            let b = InputBody {
                id,
                node: r.id,
                focus,
                name: decode(&a0),
                protected: st.contains_key("protected"),
                value,
                caret,
                fg: text_fg(control_fg),
                graphical: control.map(|_| opts.graphical.map_or(Rgb(128, 128, 128), |c| c.placeholder)),
            };
            let nat = if b.graphical.is_some() {
                // Room for the value, or for the name when empty, and a sixteen-cell field.
                (b.value.chars().count() + 1).max(b.name.chars().count() + 1).max(16)
            } else {
                b.displayed().0.len()
            };
            Ok((Body::Input(b), nat))
        }
        t => Err(format!("cannot lay out element `{}`", t)),
    }
}

// ───────────────────────── Text helpers ─────────────────────────

/// Word wrap. Whitespace runs collapse to one space; `\n` starts a new paragraph;
/// words longer than `w` are split.
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

/// Character-exact wrap used for inputs: the ranges cover every char exactly once, so a
/// caret index maps to one range. A line breaks after the last space that fits, or hard
/// at `w` when there is none. A space right after a full line hangs at its end (it is
/// part of the range but not displayed).
fn wrap_ranges(chars: &[char], w: usize) -> Vec<(usize, usize)> {
    let w = w.max(1);
    let n = chars.len();
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut s = 0usize;
    while s < n {
        if n - s <= w {
            out.push((s, n));
            break;
        }
        let e = if chars[s + w] == ' ' {
            s + w + 1
        } else if let Some(p) = (s + 1..s + w).rev().find(|&p| chars[p] == ' ') {
            p + 1
        } else {
            s + w
        };
        out.push((s, e));
        s = e;
    }
    if out.is_empty() {
        out.push((0, 0));
    }
    out
}

fn offset(a: Align, gap: usize) -> usize {
    match a {
        Align::Start => 0,
        Align::Center => gap / 2,
        Align::End => gap,
    }
}

/// One visual line of a leaf, before it is turned into a row.
struct Line {
    text: String,
    copy: String,
    /// Caret column within `text`.
    caret: Option<usize>,
}

/// `scale` is the font size the text is drawn at: a line holds `cw / scale` chars.
fn text_lines(t: &TextBody, cw: usize, scale: usize) -> Vec<Line> {
    if t.verbatim {
        return t
            .text
            .lines()
            .map(|l| {
                let s: String = l.chars().take(cw).collect();
                Line {
                    copy: s.clone(),
                    text: s,
                    caret: None,
                }
            })
            .collect();
    }
    let inner = (cw / scale.max(1)).max(1);
    wrap(&t.text, inner)
        .into_iter()
        .map(|l| {
            let text: String = l.chars().take(inner).collect();
            let copy = t.copy.clone().unwrap_or(l);
            Line {
                text,
                copy,
                caret: None,
            }
        })
        .collect()
}

fn input_lines(b: &InputBody, cw: usize) -> (Vec<Line>, InputLayout) {
    let (disp, prefix) = b.displayed();
    let segs = wrap_ranges(&disp, cw);
    let vlen = b.value.chars().count();
    let caret_at = b.caret.map(|c| prefix + c.min(vlen));
    let placeholder: Vec<char> = if b.placeholder() { b.name.chars().collect() } else { vec![] };
    let lines = segs
        .iter()
        .map(|&(s, e)| {
            let text: String = if placeholder.is_empty() {
                disp[s..e].iter().take(cw).collect()
            } else {
                placeholder.iter().take(cw).collect()
            };
            let caret = caret_at.and_then(|d| {
                if d >= s && d < e {
                    Some((d - s).min(cw.saturating_sub(1)))
                } else {
                    None
                }
            });
            Line {
                text,
                copy: b.value.clone(),
                caret,
            }
        })
        .collect();
    (
        lines,
        InputLayout {
            value: b.value.clone(),
            prefix,
            segs,
            protected: b.protected,
            node: b.node,
        },
    )
}

// ───────────────────────── Place: items -> rows ─────────────────────────

struct Block {
    /// Width including margins.
    w: usize,
    rows: Vec<Row>,
    /// Shapes, relative to the block's top left.
    decos: Vec<Deco>,
}

fn space(n: usize, bg: Option<Rgb>) -> Span {
    Span {
        text: " ".repeat(n),
        fg: None,
        bg,
        bold: false,
        underline: false,
        scale: 1,
    }
}

fn push_space(spans: &mut Vec<Span>, n: usize, bg: Option<Rgb>) {
    if n > 0 {
        spans.push(space(n, bg));
    }
}

fn blank(w: usize, bg: Option<Rgb>) -> Row {
    let mut spans = Vec::new();
    push_space(&mut spans, w, bg);
    Row {
        spans,
        info: None,
        cont: false,
    }
}

fn pad_row(row: Row, left: usize, right: usize, bg: Option<Rgb>) -> Row {
    let mut spans = Vec::new();
    push_space(&mut spans, left, bg);
    spans.extend(row.spans);
    push_space(&mut spans, right, bg);
    Row {
        spans,
        info: row.info,
        cont: row.cont,
    }
    .shifted(left)
}

/// A border cell. A window draws its border as a shape, so the cell is a blank gutter.
fn edge(it: &Item, text: String) -> Span {
    let text = if it.gfx.as_ref().is_some_and(|g| g.gutter) {
        " ".repeat(text.chars().count())
    } else {
        text
    };
    Span {
        text,
        fg: it.border_fg,
        bg: it.bg,
        bold: false,
        underline: false,
        scale: 1,
    }
}

/// Wraps a content row in the element's margin, border and padding.
fn frame_row(it: &Item, inner: Row) -> Row {
    let [_, mr, _, ml] = it.margin;
    let [_, pr, _, pl] = it.pad;
    let [_, br, _, bl] = it.border;
    let mut spans = Vec::new();
    push_space(&mut spans, ml, it.outer_bg);
    if bl > 0 {
        spans.push(edge(it, "\u{2502}".to_string()));
    }
    push_space(&mut spans, pl, it.frame_bg);
    spans.extend(inner.spans);
    push_space(&mut spans, pr, it.frame_bg);
    if br > 0 {
        spans.push(edge(it, "\u{2502}".to_string()));
    }
    push_space(&mut spans, mr, it.outer_bg);
    Row {
        spans,
        info: inner.info,
        cont: inner.cont,
    }
    .shifted(ml + bl + pl)
}

/// Top or bottom border line, with corners only where a side border meets it.
fn horizontal(it: &Item, cw: usize, left: &str, right: &str) -> Row {
    let (left, right) = if it.round {
        (round_corner(left), round_corner(right))
    } else {
        (left, right)
    };
    let [_, mr, _, ml] = it.margin;
    let [_, pr, _, pl] = it.pad;
    let [_, br, _, bl] = it.border;
    let mut text = String::new();
    if bl > 0 {
        text.push_str(left);
    }
    text.push_str(&"\u{2500}".repeat(pl + cw + pr));
    if br > 0 {
        text.push_str(right);
    }
    let mut spans = Vec::new();
    push_space(&mut spans, ml, it.outer_bg);
    spans.push(edge(it, text));
    push_space(&mut spans, mr, it.outer_bg);
    Row {
        spans,
        info: None,
        cont: false,
    }
}

/// The rounded form of a square corner character.
fn round_corner(c: &str) -> &str {
    match c {
        "\u{250C}" => "\u{256D}",
        "\u{2510}" => "\u{256E}",
        "\u{2514}" => "\u{2570}",
        "\u{2518}" => "\u{256F}",
        o => o,
    }
}

fn leaf_row(it: &Item, cw: usize, line: Line, look: &Look) -> Row {
    let chars = line.text.chars().count();
    let len = chars * look.scale;
    let gap = cw.saturating_sub(len);
    let a = if look.verbatim { Align::Start } else { it.ax };
    let left = offset(a, gap);
    let mut spans = Vec::new();
    push_space(&mut spans, left, it.bg);
    if chars > 0 {
        spans.push(Span {
            text: line.text,
            fg: look.fg,
            bg: it.bg,
            bold: look.bold,
            underline: look.underline,
            scale: look.scale as u8,
        });
    }
    push_space(&mut spans, gap - left, it.bg);
    Row {
        spans,
        info: Some(LineInfo {
            kind: look.kind,
            text: line.copy,
            cols: (left, left + len),
            focus: look.focus,
            input: look.input,
            caret: line.caret.map(|c| left + c),
            href: look.href.clone(),
            node: look.node,
        }),
        cont: false,
    }
}

/// Turns lines into rows. When the height is fixed and shorter than the text, the window
/// follows the caret (or shows the top when nothing is being edited). A scaled line is
/// followed by `scale - 1` continuation rows.
fn leaf_rows(
    it: &Item,
    cw: usize,
    lines: Vec<Line>,
    look: &Look,
    fixed_ch: Option<usize>,
) -> Vec<Row> {
    let mut start = 0usize;
    let mut count = lines.len();
    if let Some(ch) = fixed_ch {
        if look.scale == 1 && lines.len() > ch {
            let caret_line = lines.iter().position(|l| l.caret.is_some());
            start = caret_line.map_or(0, |i| (i + 1).saturating_sub(ch));
            count = ch;
        }
    }
    let mut out = Vec::new();
    for l in lines.into_iter().skip(start).take(count) {
        out.push(leaf_row(it, cw, l, look));
        for _ in 1..look.scale {
            let mut r = blank(cw, it.bg);
            r.cont = true;
            out.push(r);
        }
    }
    out
}

/// Lays out `it` inside `aw` columns (and `ah` rows when the parent's height is known).
fn place(it: &Item, aw: usize, ah: Option<usize>, st: &mut State) -> Block {
    let [mt, mr, mb, ml] = it.margin;
    let [pt, _, pb, _] = it.pad;
    let [bt, _, bb, _] = it.border;
    let chrome_h = it.chrome_h();
    let chrome_v = pt + pb + bt + bb;

    let avail_bw = aw.saturating_sub(ml + mr);
    let bw = match it.size.w {
        Dim::Cells(n) => n.min(avail_bw),
        Dim::Max => avail_bw,
        Dim::Min => (it.nat + chrome_h).min(avail_bw),
    };
    let mut cw = bw.saturating_sub(chrome_h);
    if cw == 0 && it.nat > 0 {
        cw = 1;
    }
    let bw = cw + chrome_h;

    let avail_bh = ah.map(|a| a.saturating_sub(mt + mb));
    let fixed_ch = match it.size.h {
        Dim::Cells(n) => Some(n.saturating_sub(chrome_v)),
        Dim::Max => avail_bh.map(|a| a.saturating_sub(chrome_v)),
        Dim::Min => None,
    };

    let mut inner: Vec<Row> = Vec::new();
    let mut kid_decos: Vec<Deco> = Vec::new();
    match &it.body {
        Body::Box(kids) => {
            for k in kids {
                let remaining = fixed_ch.map(|c| c.saturating_sub(inner.len()));
                let blk = place(k, cw, remaining, st);
                let gap = cw.saturating_sub(blk.w);
                let left = offset(k.ax, gap);
                kid_decos.extend(shifted(blk.decos, left, inner.len()));
                for r in blk.rows {
                    inner.push(pad_row(r, left, gap - left, it.bg));
                }
            }
        }
        Body::Text(t) => {
            // Never scale beyond the width available.
            let scale = t.scale.clamp(1, cw.max(1));
            let mut look = t.look();
            look.scale = scale;
            inner = leaf_rows(it, cw, text_lines(t, cw, scale), &look, fixed_ch);
        }
        Body::Input(b) => {
            let (lines, il) = input_lines(b, cw);
            st.inputs[b.id] = il;
            inner = leaf_rows(it, cw, lines, &b.look(), fixed_ch);
        }
    }
    if let Some(ch) = fixed_ch {
        inner.truncate(ch);
        while inner.len() < ch {
            inner.push(blank(cw, it.bg));
        }
    }

    let w = bw + ml + mr;
    let mut out: Vec<Row> = Vec::new();
    for _ in 0..mt {
        out.push(blank(w, it.outer_bg));
    }
    if bt > 0 {
        out.push(horizontal(it, cw, "\u{250C}", "\u{2510}"));
    }
    for _ in 0..pt {
        out.push(frame_row(it, blank(cw, it.frame_bg)));
    }
    for r in inner {
        out.push(frame_row(it, r));
    }
    for _ in 0..pb {
        out.push(frame_row(it, blank(cw, it.frame_bg)));
    }
    if bb > 0 {
        out.push(horizontal(it, cw, "\u{2514}", "\u{2518}"));
    }
    for _ in 0..mb {
        out.push(blank(w, it.outer_bg));
    }
    // The shape of this element goes behind the shapes of what is inside it.
    let box_rows = out.len() - mt - mb;
    let mut decos: Vec<Deco> = Vec::new();
    if let Some(g) = &it.gfx {
        decos.push(g.deco(ml, mt, bw, box_rows));
    }
    decos.extend(shifted(kid_decos, ml + it.border[3] + it.pad[3], mt + bt + pt));
    Block { w, rows: out, decos }
}

#[cfg(test)]
mod tests;
