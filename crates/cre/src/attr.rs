//! Attributes. [`Attr`] is the whitelist: a key that is not a variant does not compile,
//! and rustc suggests the closest one. Values are written as strings in pages and turned
//! into the right type with `Into`; a bad value is kept as `Invalid(text)` so the browser
//! reports it when the page is drawn (the same message a bad value always got).

use std::{fmt, rc::Rc};

use crate::{Element, Root};

fn split(s: &str) -> Vec<&str> {
    s.split(',').map(str::trim).collect()
}

// ── Color ───────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Color {
    Black,
    White,
    Red,
    Green,
    Blue,
    Yellow,
    Cyan,
    Magenta,
    Gray,
    Rgb(u8, u8, u8),
    /// Not a color name or `#rgb` / `#rrggbb`; carries the text as written.
    Invalid(String),
}

impl From<&str> for Color {
    fn from(s: &str) -> Color {
        let t = s.trim();
        match t {
            "black" => Color::Black,
            "white" => Color::White,
            "red" => Color::Red,
            "green" => Color::Green,
            "blue" => Color::Blue,
            "yellow" => Color::Yellow,
            "cyan" => Color::Cyan,
            "magenta" => Color::Magenta,
            "gray" | "grey" => Color::Gray,
            _ => parse_hex(t).unwrap_or_else(|| Color::Invalid(s.to_string())),
        }
    }
}

fn parse_hex(t: &str) -> Option<Color> {
    let h = t.strip_prefix('#')?;
    if !h.is_ascii() {
        return None;
    }
    let p = |x: &str| u8::from_str_radix(x, 16).ok();
    match h.len() {
        6 => Some(Color::Rgb(p(&h[0..2])?, p(&h[2..4])?, p(&h[4..6])?)),
        3 => Some(Color::Rgb(p(&h[0..1])? * 17, p(&h[1..2])? * 17, p(&h[2..3])? * 17)),
        _ => None,
    }
}

impl Color {
    pub fn is_valid(&self) -> bool {
        !matches!(self, Color::Invalid(_))
    }

    /// The text the browser parses: a name or `#rrggbb`.
    pub fn value(&self) -> String {
        match self {
            Color::Black => "black".into(),
            Color::White => "white".into(),
            Color::Red => "red".into(),
            Color::Green => "green".into(),
            Color::Blue => "blue".into(),
            Color::Yellow => "yellow".into(),
            Color::Cyan => "cyan".into(),
            Color::Magenta => "magenta".into(),
            Color::Gray => "gray".into(),
            Color::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
            Color::Invalid(s) => s.clone(),
        }
    }
}

// ── Pad (also used for margin) ──────────────────────────────────────

/// Spacing in cells: `"1"` all sides, `"1,2"` x,y, `"1,0,1,2"` top,right,bottom,left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pad {
    All(u16),
    Xy(u16, u16),
    Sides(u16, u16, u16, u16),
    Invalid(String),
}

impl From<&str> for Pad {
    fn from(s: &str) -> Pad {
        let nums: Option<Vec<u16>> = split(s).iter().map(|p| p.parse().ok()).collect();
        match nums.as_deref() {
            Some([a]) => Pad::All(*a),
            Some([x, y]) => Pad::Xy(*x, *y),
            Some([t, r, b, l]) => Pad::Sides(*t, *r, *b, *l),
            _ => Pad::Invalid(s.to_string()),
        }
    }
}

impl Pad {
    pub fn is_valid(&self) -> bool {
        !matches!(self, Pad::Invalid(_))
    }

    pub fn values(&self) -> Vec<String> {
        match self {
            Pad::All(a) => vec![a.to_string()],
            Pad::Xy(x, y) => vec![x.to_string(), y.to_string()],
            Pad::Sides(t, r, b, l) => {
                vec![t.to_string(), r.to_string(), b.to_string(), l.to_string()]
            }
            Pad::Invalid(s) => split(s).into_iter().map(String::from).collect(),
        }
    }
}

// ── Border ──────────────────────────────────────────────────────────

/// A terminal border is one cell or none: `"1"`, `"0"`, `"1,0"` (x,y) or four values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Border {
    Off,
    On,
    Xy(bool, bool),
    Sides(bool, bool, bool, bool),
    Invalid(String),
}

impl From<&str> for Border {
    fn from(s: &str) -> Border {
        let bits: Option<Vec<bool>> = split(s)
            .iter()
            .map(|p| match *p {
                "0" => Some(false),
                "1" => Some(true),
                _ => None,
            })
            .collect();
        match bits.as_deref() {
            Some([false]) => Border::Off,
            Some([true]) => Border::On,
            Some([x, y]) => Border::Xy(*x, *y),
            Some([t, r, b, l]) => Border::Sides(*t, *r, *b, *l),
            _ => Border::Invalid(s.to_string()),
        }
    }
}

impl Border {
    pub fn is_valid(&self) -> bool {
        !matches!(self, Border::Invalid(_))
    }

    pub fn values(&self) -> Vec<String> {
        let n = |b: &bool| if *b { "1" } else { "0" }.to_string();
        match self {
            Border::Off => vec!["0".into()],
            Border::On => vec!["1".into()],
            Border::Xy(x, y) => vec![n(x), n(y)],
            Border::Sides(t, r, b, l) => vec![n(t), n(r), n(b), n(l)],
            Border::Invalid(s) => split(s).into_iter().map(String::from).collect(),
        }
    }
}

// ── Radius and Fade ─────────────────────────────────────────────────

/// How round the corners of a box are. A terminal draws `Radius::None` as square corners
/// and anything else as rounded ones; a window draws the real radius.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Radius {
    None,
    Sm,
    Md,
    Lg,
    Xl,
    /// Fully rounded ends (a pill).
    Full,
    /// A size in points.
    Px(u16),
    Invalid(String),
}

impl From<&str> for Radius {
    fn from(s: &str) -> Radius {
        let t = s.trim();
        match t {
            "none" => Radius::None,
            "sm" => Radius::Sm,
            "md" => Radius::Md,
            "lg" => Radius::Lg,
            "xl" => Radius::Xl,
            "full" | "pill" => Radius::Full,
            _ => match t.strip_suffix("px").unwrap_or(t).parse::<u16>() {
                Ok(0) => Radius::None,
                Ok(n) if n <= 999 => Radius::Px(n),
                _ => Radius::Invalid(s.to_string()),
            },
        }
    }
}

impl Radius {
    pub fn is_valid(&self) -> bool {
        !matches!(self, Radius::Invalid(_))
    }

    pub fn value(&self) -> String {
        match self {
            Radius::None => "none".into(),
            Radius::Sm => "sm".into(),
            Radius::Md => "md".into(),
            Radius::Lg => "lg".into(),
            Radius::Xl => "xl".into(),
            Radius::Full => "full".into(),
            Radius::Px(n) => n.to_string(),
            Radius::Invalid(s) => s.clone(),
        }
    }
}

/// Which side a border is strongest on; it fades away toward the opposite side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fade {
    None,
    Top,
    Right,
    Bottom,
    Left,
    /// The whole border, softened.
    Soft,
    Invalid(String),
}

impl From<&str> for Fade {
    fn from(s: &str) -> Fade {
        match s.trim() {
            "none" | "0" | "off" => Fade::None,
            "top" => Fade::Top,
            "right" => Fade::Right,
            "bottom" => Fade::Bottom,
            "left" => Fade::Left,
            "soft" | "all" => Fade::Soft,
            _ => Fade::Invalid(s.to_string()),
        }
    }
}

impl Fade {
    pub fn is_valid(&self) -> bool {
        !matches!(self, Fade::Invalid(_))
    }

    pub fn value(&self) -> String {
        match self {
            Fade::None => "none".into(),
            Fade::Top => "top".into(),
            Fade::Right => "right".into(),
            Fade::Bottom => "bottom".into(),
            Fade::Left => "left".into(),
            Fade::Soft => "soft".into(),
            Fade::Invalid(s) => s.clone(),
        }
    }
}

// ── String conversions ──────────────────────────────────────────────

macro_rules! from_text {
    ($($t:ty),*) => {$(
        impl From<String> for $t { fn from(s: String) -> $t { <$t>::from(s.as_str()) } }
        impl From<&String> for $t { fn from(s: &String) -> $t { <$t>::from(s.as_str()) } }
    )*};
}
from_text!(Color, Pad, Border, InputType, Radius, Fade);

// ── InputType ───────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputType {
    Text,
    /// Shown as `*`s; copying still yields the value.
    Password,
}

impl From<&str> for InputType {
    /// Anything that is not `password` is plain text.
    fn from(s: &str) -> InputType {
        match s.trim().to_ascii_lowercase().as_str() {
            "password" => InputType::Password,
            _ => InputType::Text,
        }
    }
}

// ── Handler ─────────────────────────────────────────────────────────

/// What a button runs. `this` is the element that was clicked; `root` is the page.
#[derive(Clone)]
pub struct Handler(pub(crate) Rc<dyn Fn(&Element, &Root)>);

impl<F: Fn(&Element, &Root) + 'static> From<F> for Handler {
    fn from(f: F) -> Handler {
        Handler(Rc::new(f))
    }
}

impl fmt::Debug for Handler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Handler")
    }
}

// ── Attr ────────────────────────────────────────────────────────────

/// Every attribute a tag takes. Written `name: value` in a macro's `{ }`.
#[allow(non_camel_case_types)]
#[derive(Clone, Debug)]
pub enum Attr {
    bcolor(Color),
    fcolor(Color),
    border_color(Color),
    pad(Pad),
    pad_x(String),
    pad_y(String),
    margin(Pad),
    margin_x(String),
    margin_y(String),
    border(Border),
    /// Rounded corners (`"md"`, `"full"`, `"12"` ...). Windows draw them; a terminal draws
    /// rounded corner characters on bordered boxes.
    border_radius(Radius),
    /// The side a border is strongest on (`"bottom"`): it fades out toward the other.
    /// Windows only.
    border_fading(Fade),
    size(String),
    size_x(String),
    size_y(String),
    align(String),
    align_x(String),
    id(String),
    /// Take the style (and `before` / `after`) of a template box; its children are not
    /// copied.
    extends(Element),
    /// Content placed before the element's own children.
    before(Element),
    /// Content placed after the element's own children.
    after(Element),
    onclick(Handler),
}

impl From<&Element> for Element {
    fn from(e: &Element) -> Element {
        e.clone()
    }
}

impl Attr {
    /// The style key this attribute is stored under (`border_color` is `border-color`).
    pub fn key(&self) -> &'static str {
        match self {
            Attr::bcolor(_) => "bcolor",
            Attr::fcolor(_) => "fcolor",
            Attr::border_color(_) => "border-color",
            Attr::pad(_) => "pad",
            Attr::pad_x(_) => "pad-x",
            Attr::pad_y(_) => "pad-y",
            Attr::margin(_) => "margin",
            Attr::margin_x(_) => "margin-x",
            Attr::margin_y(_) => "margin-y",
            Attr::border(_) => "border",
            Attr::border_radius(_) => "border-radius",
            Attr::border_fading(_) => "border-fading",
            Attr::size(_) => "size",
            Attr::size_x(_) => "size-x",
            Attr::size_y(_) => "size-y",
            Attr::align(_) => "align",
            Attr::align_x(_) => "align-x",
            Attr::id(_) => "id",
            Attr::extends(_) => "extends",
            Attr::before(_) => "before",
            Attr::after(_) => "after",
            Attr::onclick(_) => "onclick",
        }
    }

    /// The style values for plain attributes; `None` for the structural ones (`extends`,
    /// `before`, `after`, `onclick`).
    pub fn values(&self) -> Option<Vec<String>> {
        let list = |s: &str| split(s).into_iter().map(String::from).collect();
        Some(match self {
            Attr::bcolor(c) | Attr::fcolor(c) | Attr::border_color(c) => vec![c.value()],
            Attr::pad(p) | Attr::margin(p) => p.values(),
            Attr::border(b) => b.values(),
            Attr::border_radius(r) => vec![r.value()],
            Attr::border_fading(f) => vec![f.value()],
            Attr::pad_x(s)
            | Attr::pad_y(s)
            | Attr::margin_x(s)
            | Attr::margin_y(s)
            | Attr::size(s)
            | Attr::size_x(s)
            | Attr::size_y(s)
            | Attr::align(s)
            | Attr::align_x(s) => list(s),
            Attr::id(s) => vec![s.clone()],
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse_from_strings() {
        assert_eq!(Color::from("white"), Color::White);
        assert_eq!(Color::from("#181818"), Color::Rgb(0x18, 0x18, 0x18));
        assert_eq!(Color::from("#fff"), Color::Rgb(255, 255, 255));
        assert_eq!(Color::from("grey"), Color::Gray);
        assert_eq!(Color::from("nope"), Color::Invalid("nope".into()));
        assert!(!Color::from("#12").is_valid());
        assert_eq!(Color::Rgb(1, 2, 255).value(), "#0102ff");
        assert_eq!(Color::from("nope").value(), "nope");
    }

    #[test]
    fn pad_shorthand() {
        assert_eq!(Pad::from("2"), Pad::All(2));
        assert_eq!(Pad::from("2,1"), Pad::Xy(2, 1));
        assert_eq!(Pad::from("1, 0, 1, 2"), Pad::Sides(1, 0, 1, 2));
        assert_eq!(Pad::from("1,0,1,2").values(), ["1", "0", "1", "2"]);
        assert!(!Pad::from("1,2,3").is_valid());
        assert!(!Pad::from("-1").is_valid());
        assert!(!Pad::from("wide").is_valid());
    }

    #[test]
    fn border_is_on_or_off() {
        assert_eq!(Border::from("1"), Border::On);
        assert_eq!(Border::from("0"), Border::Off);
        assert_eq!(Border::from("1,0"), Border::Xy(true, false));
        assert_eq!(Border::from("1,0,0,1").values(), ["1", "0", "0", "1"]);
        assert!(!Border::from("2").is_valid());
    }

    #[test]
    fn radius_and_fade_parse_from_strings() {
        assert_eq!(Radius::from("md"), Radius::Md);
        assert_eq!(Radius::from("pill"), Radius::Full);
        assert_eq!(Radius::from("12px"), Radius::Px(12));
        assert_eq!(Radius::from("0"), Radius::None);
        assert!(!Radius::from("round").is_valid());
        assert_eq!(Radius::from("12px").value(), "12");
        assert_eq!(Fade::from("bottom"), Fade::Bottom);
        assert!(!Fade::from("diagonal").is_valid());
        assert_eq!(Attr::border_radius("lg".into()).key(), "border-radius");
        assert_eq!(Attr::border_fading("top".into()).values(), Some(vec!["top".to_string()]));
    }

    #[test]
    fn input_type_from_text() {
        assert_eq!(InputType::from("Password"), InputType::Password);
        assert_eq!(InputType::from("whatever"), InputType::Text);
    }

    #[test]
    fn attr_keys_use_hyphens_and_values_split_on_commas() {
        let a = Attr::border_color("cyan".into());
        assert_eq!(a.key(), "border-color");
        assert_eq!(a.values(), Some(vec!["cyan".to_string()]));
        assert_eq!(Attr::size("20, min".into()).values().unwrap(), ["20", "min"]);
        assert_eq!(Attr::align_x("middle".into()).key(), "align-x");
        assert!(Attr::onclick((|_: &Element, _: &Root| {}).into()).values().is_none());
    }
}
