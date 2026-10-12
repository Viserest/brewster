//! Cell-grid geometry shared by GUI backends. The page is laid out in terminal-style
//! cells; a backend measures one cell and these helpers keep every cell edge on a whole
//! physical pixel, so neighbouring fills never leave seams (the "grid lines" between rows
//! of a colored background) and text always sits inside its cell.
//! No windowing types here, so it is unit-tested.

use engine::Rgb;

/// Colors used where a cell has none of its own.
pub const THEME_BG: Rgb = Rgb(24, 24, 24);
pub const THEME_FG: Rgb = Rgb(220, 220, 220);

/// Size of one character cell in points, always a whole number of physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub cell_w: f32,
    pub cell_h: f32,
    /// Physical pixels per point.
    pub ppp: f32,
}

fn sane_ppp(ppp: f32) -> f32 {
    if ppp.is_finite() && ppp > 0.0 {
        ppp
    } else {
        1.0
    }
}

/// Rounds a length in points to a whole number of physical pixels.
pub fn snap(v: f32, ppp: f32) -> f32 {
    let ppp = sane_ppp(ppp);
    (v * ppp).round() / ppp
}

impl Metrics {
    /// From the measured advance of one glyph and the height of one line, in points. The
    /// width is rounded to the nearest pixel (text is placed per cell, so a fraction of a
    /// pixel of difference never accumulates along a line) and the height is rounded up so
    /// a line never touches the next.
    pub fn measured(advance: f32, line_h: f32, ppp: f32) -> Metrics {
        let ppp = sane_ppp(ppp);
        let w = if advance.is_finite() {
            (advance * ppp).round().max(1.0)
        } else {
            1.0
        };
        let h = if line_h.is_finite() {
            (line_h * ppp).ceil().max(1.0)
        } else {
            1.0
        };
        Metrics {
            cell_w: w / ppp,
            cell_h: h / ppp,
            ppp,
        }
    }

    /// One physical pixel in points.
    pub fn px(&self) -> f32 {
        1.0 / sane_ppp(self.ppp)
    }
}

/// How many whole cells fit in `w` x `h` points: `(columns, rows)`, at least 1 x 1.
pub fn grid_size(w: f32, h: f32, m: Metrics) -> (usize, usize) {
    if !(m.cell_w > 0.0 && m.cell_h > 0.0) {
        return (1, 1);
    }
    // A tiny tolerance: 800 / 8 must be 100 cells even when the division comes out 99.99999.
    let cols = (w / m.cell_w + 1e-3).floor().max(1.0);
    let rows = (h / m.cell_h + 1e-3).floor().max(1.0);
    (cols as usize, rows as usize)
}

/// Point x of the left edge of column `col`.
pub fn x_of(origin_x: f32, col: usize, m: Metrics) -> f32 {
    snap(origin_x + col as f32 * m.cell_w, m.ppp)
}

/// Point y of the top edge of row `row`.
pub fn y_of(origin_y: f32, row: usize, m: Metrics) -> f32 {
    snap(origin_y + row as f32 * m.cell_h, m.ppp)
}

/// The cell under a point, or `None` when the point is above or left of the grid.
pub fn cell_at(origin_x: f32, origin_y: f32, x: f32, y: f32, m: Metrics) -> Option<(usize, usize)> {
    if !(m.cell_w > 0.0 && m.cell_h > 0.0) {
        return None;
    }
    let (dx, dy) = (x - origin_x, y - origin_y);
    if !(dx >= 0.0 && dy >= 0.0) {
        return None;
    }
    Some(((dx / m.cell_w) as usize, (dy / m.cell_h) as usize))
}

/// Foreground and background of a cell. A reversed cell swaps them, using the theme's
/// colors for whichever is unset, so reversed text is visible on any background.
pub fn resolve(fg: Option<Rgb>, bg: Option<Rgb>, reverse: bool) -> (Rgb, Option<Rgb>) {
    if reverse {
        (bg.unwrap_or(THEME_BG), Some(fg.unwrap_or(THEME_FG)))
    } else {
        (fg.unwrap_or(THEME_FG), bg)
    }
}

/// Turns wheel movement (points, positive = wheel up) into scroll events: one event per
/// three lines of movement, because the browser scrolls three lines per event. Small
/// movements (a trackpad) add up instead of each scrolling a whole step.
#[derive(Debug, Default, Clone, Copy)]
pub struct Wheel {
    acc: f32,
}

impl Wheel {
    /// Returns whole events to send: positive = scroll down, negative = scroll up.
    pub fn push(&mut self, raw_y: f32, cell_h: f32) -> isize {
        if raw_y == 0.0 || raw_y.is_nan() || !(cell_h > 0.0) {
            return 0;
        }
        let step = cell_h * 3.0;
        // Moving the other way starts afresh.
        if (self.acc > 0.0) != (raw_y > 0.0) {
            self.acc = 0.0;
        }
        self.acc += raw_y;
        let n = (self.acc / step).trunc();
        self.acc -= n * step;
        (-n).clamp(-20.0, 20.0) as isize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: Metrics = Metrics {
        cell_w: 8.0,
        cell_h: 16.0,
        ppp: 1.0,
    };

    #[test]
    fn measured_metrics_are_whole_pixels() {
        let m = Metrics::measured(8.4, 17.2, 1.0);
        assert_eq!((m.cell_w, m.cell_h), (8.0, 18.0));
        // On a 1.5x display a cell is a whole number of physical pixels: 13/1.5, 26/1.5.
        let m = Metrics::measured(8.4, 17.2, 1.5);
        assert!((m.cell_w * 1.5 - 13.0).abs() < 1e-4, "{}", m.cell_w);
        assert!((m.cell_h * 1.5 - 26.0).abs() < 1e-4, "{}", m.cell_h);
        assert!((m.px() - 1.0 / 1.5).abs() < 1e-6);
    }

    #[test]
    fn broken_measurements_do_not_break_the_grid() {
        let m = Metrics::measured(f32::NAN, 0.0, 0.0);
        assert!(m.cell_w > 0.0 && m.cell_h > 0.0 && m.ppp == 1.0);
        assert_eq!(Metrics::measured(0.0, 0.0, 2.0).cell_w, 0.5);
    }

    #[test]
    fn cell_edges_fall_on_pixels_so_fills_leave_no_seams() {
        for ppp in [1.0_f32, 1.25, 1.5, 2.0, 2.5] {
            let m = Metrics::measured(8.37, 17.9, ppp);
            for col in 0..300 {
                let right = x_of(3.3, col, m) + m.cell_w;
                let next = x_of(3.3, col + 1, m);
                assert!(
                    (right - next).abs() < 1e-3,
                    "ppp {} col {}: {} vs {}",
                    ppp,
                    col,
                    right,
                    next
                );
                let px = x_of(3.3, col, m) * ppp;
                assert!((px - px.round()).abs() < 1e-3, "x not on a pixel: {}", px);
            }
            for row in 0..200 {
                let bottom = y_of(0.0, row, m) + m.cell_h;
                let next = y_of(0.0, row + 1, m);
                assert!((bottom - next).abs() < 1e-3, "ppp {} row {}", ppp, row);
            }
        }
    }

    #[test]
    fn grid_size_counts_whole_cells() {
        assert_eq!(grid_size(800.0, 600.0, M), (100, 37));
        assert_eq!(grid_size(807.9, 623.0, M), (100, 38));
        assert_eq!(grid_size(799.9999, 600.0, M).0, 100);
    }

    #[test]
    fn grid_size_is_never_empty_or_broken() {
        assert_eq!(grid_size(3.0, 3.0, M), (1, 1));
        assert_eq!(grid_size(0.0, 0.0, M), (1, 1));
        assert_eq!(grid_size(-50.0, -50.0, M), (1, 1));
        assert_eq!(
            grid_size(100.0, 100.0, Metrics { cell_w: 0.0, ..M }),
            (1, 1)
        );
        assert_eq!(grid_size(f32::NAN, 100.0, M).0, 1);
    }

    #[test]
    fn positions_follow_the_grid() {
        assert_eq!(x_of(10.0, 3, M), 34.0);
        assert_eq!(y_of(5.0, 2, M), 37.0);
    }

    #[test]
    fn points_map_to_cells() {
        assert_eq!(cell_at(10.0, 5.0, 10.0, 5.0, M), Some((0, 0)));
        assert_eq!(cell_at(10.0, 5.0, 34.5, 37.0, M), Some((3, 2)));
        assert_eq!(cell_at(10.0, 5.0, 9.9, 6.0, M), None);
        assert_eq!(cell_at(10.0, 5.0, 12.0, 4.0, M), None);
        assert_eq!(cell_at(0.0, 0.0, f32::NAN, 1.0, M), None);
    }

    #[test]
    fn reversed_cells_swap_and_fall_back_to_the_theme() {
        let white = Rgb(255, 255, 255);
        let black = Rgb(0, 0, 0);
        assert_eq!(
            resolve(Some(black), Some(white), false),
            (black, Some(white))
        );
        assert_eq!(
            resolve(Some(black), Some(white), true),
            (white, Some(black))
        );
        assert_eq!(resolve(None, None, false), (THEME_FG, None));
        assert_eq!(resolve(None, None, true), (THEME_BG, Some(THEME_FG)));
        assert_eq!(resolve(Some(black), None, true), (THEME_BG, Some(black)));
    }

    #[test]
    fn wheel_up_scrolls_up_and_down_scrolls_down() {
        let mut w = Wheel::default();
        assert_eq!(w.push(48.0, 16.0), -1);
        assert_eq!(w.push(-48.0, 16.0), 1);
        assert_eq!(w.push(160.0, 16.0), -3);
    }

    #[test]
    fn small_wheel_movements_add_up() {
        let mut w = Wheel::default();
        let total: isize = (0..10).map(|_| w.push(-10.0, 16.0)).sum();
        assert_eq!(total, 2, "100 points of movement is two steps of 48");
        // The 4 points left over join the next movement.
        assert_eq!(w.push(-44.0, 16.0), 1);
    }

    #[test]
    fn wheel_ignores_nonsense_and_limits_bursts() {
        let mut w = Wheel::default();
        assert_eq!(w.push(0.0, 16.0), 0);
        assert_eq!(w.push(f32::NAN, 16.0), 0);
        assert_eq!(w.push(10.0, 0.0), 0);
        assert_eq!(Wheel::default().push(1e9, 16.0), -20);
    }

    #[test]
    fn changing_direction_drops_the_leftover() {
        let mut w = Wheel::default();
        assert_eq!(w.push(40.0, 16.0), 0);
        assert_eq!(w.push(-20.0, 16.0), 0);
        assert_eq!(w.push(-30.0, 16.0), 1, "-20 and -30 make 50, not 30 - 20");
    }
}
