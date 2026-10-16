//! A grid of styled cells: what the browser composes (tab bar, page, status line, settings
//! box) and what the terminal and window frontends paint.

use engine::{Deco, Fade, Rgb};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub underline: bool,
    /// Swap foreground and background (the theme's colors when both are `None`).
    pub reverse: bool,
    /// Font size multiplier. A cell with `scale > 1` holds a glyph that is `scale` cells
    /// wide and high, starting at this cell; only graphical frontends get such cells.
    pub scale: u8,
    /// Covered by the glyph of a scaled cell: only the background is drawn.
    pub cover: bool,
}

impl Default for Cell {
    fn default() -> Cell {
        Cell {
            ch: ' ',
            fg: None,
            bg: None,
            bold: false,
            underline: false,
            reverse: false,
            scale: 1,
            cover: false,
        }
    }
}

/// Style for text put on the screen by the browser itself.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Look {
    pub bold: bool,
    pub underline: bool,
    pub reverse: bool,
}

impl Look {
    pub const PLAIN: Look = Look { bold: false, underline: false, reverse: false };
    pub const REVERSE: Look = Look { bold: false, underline: false, reverse: true };
    pub const BOLD_REVERSE: Look = Look { bold: true, underline: false, reverse: true };
    pub const BOLD: Look = Look { bold: true, underline: false, reverse: false };
}

/// A shape a window draws behind the text: a rounded background, a border that may fade,
/// a shadow or a focus ring. Units are cells (fractions allowed), measured on the screen;
/// radii are in points. A terminal ignores shapes.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub fill: Option<Rgb>,
    pub stroke: Option<Rgb>,
    /// Which sides of the stroke are drawn: top, right, bottom, left.
    pub sides: [bool; 4],
    /// Top left, top right, bottom right and bottom left radii.
    pub radii: [f32; 4],
    pub fade: Fade,
    pub shadow: bool,
    /// A focus ring: an outline just outside the box, in `stroke`.
    pub ring: bool,
    /// Rows `[top, bottom)` the shape is cut to (the page's area).
    pub clip: Option<(f32, f32)>,
}

impl Shape {
    /// A plain rounded box.
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Shape {
        Shape {
            x,
            y,
            w,
            h,
            fill: None,
            stroke: None,
            sides: [true; 4],
            radii: [0.0; 4],
            fade: Fade::None,
            shadow: false,
            ring: false,
            clip: None,
        }
    }

    pub fn filled(mut self, fill: Rgb) -> Shape {
        self.fill = Some(fill);
        self
    }

    pub fn rounded(mut self, radius: f32) -> Shape {
        self.radii = [radius; 4];
        self
    }

    /// A page shape moved to its place on the screen: `dx` columns right and `dy` rows
    /// down, cut to the rows `clip`.
    pub fn from_deco(d: &Deco, dx: f32, dy: f32, clip: (f32, f32)) -> Shape {
        Shape {
            x: d.col as f32 + dx + d.inset.0,
            y: d.row as f32 + dy + d.inset.1,
            w: d.cols as f32 - 2.0 * d.inset.0,
            h: d.rows as f32 - 2.0 * d.inset.1,
            fill: d.fill,
            stroke: d.stroke,
            sides: d.sides,
            radii: [d.radius; 4],
            fade: d.fade,
            shadow: false,
            ring: false,
            clip: Some(clip),
        }
    }

    /// The focus ring of a shape.
    pub fn ringed(&self, color: Rgb) -> Shape {
        Shape {
            fill: None,
            stroke: Some(color),
            sides: [true; 4],
            fade: Fade::None,
            shadow: false,
            ring: true,
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    cells: Vec<Cell>,
    /// Shapes, back to front. Drawn after the cells' backgrounds and before their glyphs.
    pub shapes: Vec<Shape>,
    /// The colors of the page and bars when a theme sets them: the window behind the grid
    /// is filled with the background.
    pub bg: Option<Rgb>,
    pub fg: Option<Rgb>,
}

impl Screen {
    pub fn new(cols: usize, rows: usize) -> Screen {
        Screen {
            cols,
            rows,
            cells: vec![Cell::default(); cols * rows],
            shapes: Vec::new(),
            bg: None,
            fg: None,
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&Cell> {
        (x < self.cols && y < self.rows).then(|| &self.cells[y * self.cols + x])
    }

    pub fn set(&mut self, x: usize, y: usize, cell: Cell) {
        if x < self.cols && y < self.rows {
            self.cells[y * self.cols + x] = cell;
        }
    }

    /// Writes `text` from `(x, y)`, clipped to `max` cells, and returns the cells used.
    pub fn text(&mut self, x: usize, y: usize, text: &str, look: Look, max: usize) -> usize {
        let mut n = 0;
        for ch in text.chars() {
            if n >= max || x + n >= self.cols {
                break;
            }
            self.set(
                x + n,
                y,
                Cell { ch, bold: look.bold, underline: look.underline, reverse: look.reverse, ..Cell::default() },
            );
            n += 1;
        }
        n
    }

    /// Writes `text` in the given colors (no reverse video), clipped to `max` cells.
    pub fn text_colored(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        fg: Option<Rgb>,
        bg: Option<Rgb>,
        bold: bool,
        max: usize,
    ) -> usize {
        let mut n = 0;
        for ch in text.chars() {
            if n >= max || x + n >= self.cols {
                break;
            }
            self.set(x + n, y, Cell { ch, fg, bg, bold, ..Cell::default() });
            n += 1;
        }
        n
    }

    /// Removes the shapes that touch a rectangle of cells (something is drawn over it).
    pub fn cover(&mut self, x: usize, y: usize, w: usize, h: usize) {
        let (x0, y0, x1, y1) = (x as f32, y as f32, (x + w) as f32, (y + h) as f32);
        self.shapes
            .retain(|sh| sh.x >= x1 || sh.x + sh.w <= x0 || sh.y >= y1 || sh.y + sh.h <= y0);
    }

    /// Fills a rectangle with blank cells of one color.
    pub fn fill_color(&mut self, x: usize, y: usize, w: usize, h: usize, bg: Option<Rgb>) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, Cell { bg, ..Cell::default() });
            }
        }
    }

    /// Gives every cell the page colors where it has none of its own (a reversed cell
    /// swaps them), and remembers them in `bg` / `fg`.
    pub fn apply_colors(&mut self, bg: Rgb, fg: Rgb) {
        self.bg = Some(bg);
        self.fg = Some(fg);
        for c in &mut self.cells {
            if c.reverse {
                let (f, b) = (c.bg.unwrap_or(bg), c.fg.unwrap_or(fg));
                c.fg = Some(f);
                c.bg = Some(b);
                c.reverse = false;
            } else {
                c.fg = c.fg.or(Some(fg));
                c.bg = c.bg.or(Some(bg));
            }
        }
    }

    /// Fills a rectangle with blank cells of one look.
    pub fn fill(&mut self, x: usize, y: usize, w: usize, h: usize, look: Look) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, Cell { reverse: look.reverse, bold: look.bold, ..Cell::default() });
            }
        }
    }

    /// The text of one row, for tests and debugging.
    pub fn row_text(&self, y: usize) -> String {
        (0..self.cols).filter_map(|x| self.get(x, y)).map(|c| c.ch).collect()
    }
}
