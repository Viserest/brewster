//! A grid of styled cells: what the browser composes (tab bar, page, status line, settings
//! box) and what the terminal and window frontends paint.

use engine::Rgb;

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

#[derive(Debug, Clone, PartialEq)]
pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    cells: Vec<Cell>,
}

impl Screen {
    pub fn new(cols: usize, rows: usize) -> Screen {
        Screen { cols, rows, cells: vec![Cell::default(); cols * rows] }
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
