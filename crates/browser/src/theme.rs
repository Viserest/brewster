//! Themes: the colors and shapes of the browser's own parts (tab bar, status line, page
//! background) and how a window draws buttons, links and inputs. `tui` is the terminal
//! look the browser always had (reverse video, no shapes); the others are colored, and in
//! a window they also draw rounded shapes.
//!
//! A theme only decides how things are painted. Terminals show the colors but cannot draw
//! shapes, so a themed terminal gets flat colored tabs and plain controls.

use engine::{ControlStyle, Controls, Fade, Rgb};

/// The names `general.theme` accepts.
pub const NAMES: &[&str] = &["default", "chromium", "firefox", "tui"];

/// Corner radius that rounds a box off completely.
const FULL: f32 = 999.0;

/// Colors of the bars. `None` means reverse video: the terminal's own colors, swapped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chrome {
    pub bar_bg: Option<Rgb>,
    pub bar_fg: Option<Rgb>,
    pub status_bg: Option<Rgb>,
    pub status_fg: Option<Rgb>,
}

/// How tabs look.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabStyle {
    pub active_fill: Option<Rgb>,
    pub active_fg: Option<Rgb>,
    pub idle_fill: Option<Rgb>,
    pub idle_fg: Option<Rgb>,
    /// Corner radii in points: top left, top right, bottom right, bottom left.
    pub radii: [f32; 4],
    /// When not empty every tab gets a color from here (in turn): the active tab is filled
    /// with it, the others with a tint of it.
    pub fun: &'static [Rgb],
    /// The active tab floats above the bar on a soft shadow.
    pub shadow: bool,
    /// The close button and the new-tab button are round.
    pub round_buttons: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    /// Whether a window draws shapes (rounded tabs, pill buttons ...). Without it the
    /// colors still apply.
    pub graphical: bool,
    /// Focus rings and highlights.
    pub accent: Rgb,
    /// The colors under the page and the bars. `None`: the terminal's own.
    pub page_bg: Option<Rgb>,
    pub page_fg: Option<Rgb>,
    pub chrome: Chrome,
    pub tab: TabStyle,
    pub controls: Controls,
}

/// What a tab (or its close button) is painted with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabColors {
    pub fill: Option<Rgb>,
    pub fg: Option<Rgb>,
}

const fn rgb(v: u32) -> Rgb {
    Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// `a` mixed with `b`: `t` is the share of `a`, from 0 to 1.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |x: u8, y: u8| (f32::from(x) * t + f32::from(y) * (1.0 - t)).round().clamp(0.0, 255.0) as u8;
    Rgb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

const FUN: &[Rgb] = &[
    rgb(0x8b7cff),
    rgb(0xff7ab8),
    rgb(0x3fd6c6),
    rgb(0xffc857),
    rgb(0x7bd88f),
    rgb(0xff9f6e),
];

fn control(fg: u32, fill: Option<u32>, stroke: Option<u32>, radius: f32) -> ControlStyle {
    ControlStyle {
        fg: Some(rgb(fg)),
        fill: fill.map(rgb),
        stroke: stroke.map(rgb),
        radius,
        fade: Fade::None,
    }
}

impl Theme {
    /// A theme by name; `None` for a name that is not in [`NAMES`].
    pub fn named(name: &str) -> Option<Theme> {
        Some(match name {
            "default" => Theme::brewster(),
            "chromium" => Theme::chromium(),
            "firefox" => Theme::firefox(),
            "tui" => Theme::tui(),
            _ => return None,
        })
    }

    /// Dark, violet, round and bubbly: every tab has its own color.
    fn brewster() -> Theme {
        let accent = rgb(0x8b7cff);
        Theme {
            name: "default",
            graphical: true,
            accent,
            page_bg: Some(rgb(0x16171d)),
            page_fg: Some(rgb(0xe8e8ee)),
            chrome: Chrome {
                bar_bg: Some(rgb(0x0f1014)),
                bar_fg: Some(rgb(0xc9cad6)),
                status_bg: Some(rgb(0x0f1014)),
                status_fg: Some(rgb(0x9a9cb0)),
            },
            tab: TabStyle {
                active_fill: Some(accent),
                active_fg: Some(rgb(0x14141a)),
                idle_fill: Some(rgb(0x23242d)),
                idle_fg: Some(rgb(0xc9cad6)),
                radii: [FULL; 4],
                fun: FUN,
                shadow: true,
                round_buttons: true,
            },
            controls: Controls {
                button: control(0x101014, Some(0x8b7cff), None, FULL),
                link: control(0xc4bcff, Some(0x2a2750), None, FULL),
                input: control(0xe8e8ee, Some(0x1f2027), Some(0x3b3e4c), FULL),
                placeholder: rgb(0x7b7f8c),
            },
        }
    }

    /// Light chrome, a white tab merging into the page, blue controls.
    fn chromium() -> Theme {
        Theme {
            name: "chromium",
            graphical: true,
            accent: rgb(0x1a73e8),
            page_bg: Some(rgb(0xffffff)),
            page_fg: Some(rgb(0x202124)),
            chrome: Chrome {
                bar_bg: Some(rgb(0xdee1e6)),
                bar_fg: Some(rgb(0x3c4043)),
                status_bg: Some(rgb(0xf1f3f4)),
                status_fg: Some(rgb(0x5f6368)),
            },
            tab: TabStyle {
                active_fill: Some(rgb(0xffffff)),
                active_fg: Some(rgb(0x202124)),
                idle_fill: None,
                idle_fg: Some(rgb(0x5f6368)),
                radii: [10.0, 10.0, 0.0, 0.0],
                fun: &[],
                shadow: false,
                round_buttons: true,
            },
            controls: Controls {
                button: control(0xffffff, Some(0x1a73e8), None, FULL),
                link: control(0x1967d2, Some(0xe8f0fe), None, FULL),
                input: control(0x202124, Some(0xf1f3f4), Some(0xdadce0), FULL),
                placeholder: rgb(0x80868b),
            },
        }
    }

    /// Dark chrome, a lighter rounded tab, cyan accents, small radii.
    fn firefox() -> Theme {
        Theme {
            name: "firefox",
            graphical: true,
            accent: rgb(0x00ddff),
            page_bg: Some(rgb(0x1c1b22)),
            page_fg: Some(rgb(0xfbfbfe)),
            chrome: Chrome {
                bar_bg: Some(rgb(0x1c1b22)),
                bar_fg: Some(rgb(0xfbfbfe)),
                status_bg: Some(rgb(0x2b2a33)),
                status_fg: Some(rgb(0xcfcfd8)),
            },
            tab: TabStyle {
                active_fill: Some(rgb(0x42414d)),
                active_fg: Some(rgb(0xfbfbfe)),
                idle_fill: None,
                idle_fg: Some(rgb(0xcfcfd8)),
                radii: [6.0; 4],
                fun: &[],
                shadow: false,
                round_buttons: false,
            },
            controls: Controls {
                button: control(0x15141a, Some(0x00ddff), None, 6.0),
                link: control(0x00ddff, Some(0x2b2a33), None, 6.0),
                input: control(0xfbfbfe, Some(0x2b2a33), Some(0x5b5b66), 6.0),
                placeholder: rgb(0x9e9ea8),
            },
        }
    }

    /// The terminal look: the terminal's colors, reverse video bars, brackets and box
    /// characters.
    fn tui() -> Theme {
        let none = ControlStyle { fg: None, fill: None, stroke: None, radius: 0.0, fade: Fade::None };
        Theme {
            name: "tui",
            graphical: false,
            accent: rgb(0xffffff),
            page_bg: None,
            page_fg: None,
            chrome: Chrome { bar_bg: None, bar_fg: None, status_bg: None, status_fg: None },
            tab: TabStyle {
                active_fill: None,
                active_fg: None,
                idle_fill: None,
                idle_fg: None,
                radii: [0.0; 4],
                fun: &[],
                shadow: false,
                round_buttons: false,
            },
            controls: Controls { button: none, link: none, input: none, placeholder: rgb(0x808080) },
        }
    }

    /// Whether the bars use the theme's colors (otherwise reverse video).
    pub fn colored(&self) -> bool {
        self.chrome.bar_bg.is_some()
    }

    /// Colors for tab number `index`.
    pub fn tab_colors(&self, index: usize, active: bool) -> TabColors {
        let t = &self.tab;
        if !t.fun.is_empty() {
            let accent = t.fun[index % t.fun.len()];
            let bar = self.chrome.bar_bg.unwrap_or(rgb(0x000000));
            return if active {
                TabColors { fill: Some(accent), fg: t.active_fg }
            } else {
                TabColors { fill: Some(mix(accent, bar, 0.22)), fg: t.idle_fg }
            };
        }
        if active {
            TabColors { fill: t.active_fill, fg: t.active_fg }
        } else {
            TabColors { fill: t.idle_fill, fg: t.idle_fg }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_name_is_a_theme() {
        for n in NAMES {
            assert_eq!(Theme::named(n).unwrap().name, *n);
        }
        assert!(Theme::named("solarized").is_none());
    }

    #[test]
    fn the_tui_theme_is_the_terminals_own() {
        let t = Theme::tui();
        assert!(!t.graphical && !t.colored());
        assert_eq!((t.page_bg, t.page_fg), (None, None));
    }

    #[test]
    fn the_others_are_colored_and_graphical() {
        for n in ["default", "chromium", "firefox"] {
            let t = Theme::named(n).unwrap();
            assert!(t.graphical && t.colored(), "{n}");
            assert!(t.page_bg.is_some() && t.page_fg.is_some(), "{n}");
        }
        // Chromium is light, the others are dark.
        let light = |t: Theme| t.page_bg.map(|c| c.0 as u32 + c.1 as u32 + c.2 as u32).unwrap() > 384;
        assert!(light(Theme::chromium()));
        assert!(!light(Theme::firefox()) && !light(Theme::brewster()));
    }

    #[test]
    fn fun_tabs_cycle_through_the_palette() {
        let t = Theme::brewster();
        let n = t.tab.fun.len();
        assert_eq!(t.tab_colors(0, true).fill, t.tab_colors(n, true).fill);
        assert_ne!(t.tab_colors(0, true).fill, t.tab_colors(1, true).fill);
        // An idle tab is a tint of its color, darker than the active one.
        let (a, i) = (t.tab_colors(2, true).fill.unwrap(), t.tab_colors(2, false).fill.unwrap());
        assert!((i.0 as u32 + i.1 as u32 + i.2 as u32) < (a.0 as u32 + a.1 as u32 + a.2 as u32));
    }

    #[test]
    fn plain_themes_use_their_own_tab_colors() {
        let t = Theme::chromium();
        assert_eq!(t.tab_colors(0, true).fill, Some(rgb(0xffffff)));
        assert_eq!(t.tab_colors(3, false).fill, None);
    }

    #[test]
    fn mixing_is_linear() {
        assert_eq!(mix(Rgb(200, 0, 100), Rgb(0, 100, 0), 0.5), Rgb(100, 50, 50));
        assert_eq!(mix(Rgb(1, 2, 3), Rgb(9, 9, 9), 1.0), Rgb(1, 2, 3));
    }
}
