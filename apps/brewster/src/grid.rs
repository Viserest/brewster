//! Cell-grid geometry shared by GUI backends. The page is laid out in terminal-style
//! cells; a backend measures one cell in pixels and these helpers convert between the two.
//! No windowing types here, so it is unit-tested.

/// Size of one character cell in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub cell_w: f32,
    pub cell_h: f32,
}

/// How many whole cells fit in `w` x `h` pixels: `(columns, rows)`, at least 1 x 1.
pub fn grid_size(w: f32, h: f32, m: Metrics) -> (usize, usize) {
    if !(m.cell_w > 0.0 && m.cell_h > 0.0) {
        return (1, 1);
    }
    let cols = (w / m.cell_w).floor().max(1.0);
    let rows = (h / m.cell_h).floor().max(1.0);
    (cols as usize, rows as usize)
}

/// Pixel x of the left edge of column `col`.
pub fn x_of(origin_x: f32, col: usize, m: Metrics) -> f32 {
    origin_x + col as f32 * m.cell_w
}

/// Pixel y of the top edge of row `row` (relative to the first visible row).
pub fn y_of(origin_y: f32, row: usize, m: Metrics) -> f32 {
    origin_y + row as f32 * m.cell_h
}

/// Lines to scroll the view for a mouse-wheel delta of `raw_y` pixels (positive = wheel
/// up). Returns a negative number to scroll up, and always at least one line for any
/// non-zero delta.
pub fn scroll_lines(raw_y: f32, cell_h: f32) -> isize {
    if raw_y == 0.0 || raw_y.is_nan() || !(cell_h > 0.0) {
        return 0;
    }
    let mut lines = (raw_y / cell_h).round() as isize;
    if lines == 0 {
        lines = if raw_y > 0.0 { 1 } else { -1 };
    }
    -lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: Metrics = Metrics {
        cell_w: 8.0,
        cell_h: 16.0,
    };

    #[test]
    fn grid_size_counts_whole_cells() {
        assert_eq!(grid_size(800.0, 600.0, M), (100, 37));
        assert_eq!(grid_size(807.9, 623.0, M), (100, 38));
    }

    #[test]
    fn grid_size_is_never_empty_or_broken() {
        assert_eq!(grid_size(3.0, 3.0, M), (1, 1));
        assert_eq!(grid_size(0.0, 0.0, M), (1, 1));
        assert_eq!(grid_size(-50.0, -50.0, M), (1, 1));
        assert_eq!(grid_size(100.0, 100.0, Metrics { cell_w: 0.0, cell_h: 16.0 }), (1, 1));
        assert_eq!(grid_size(f32::NAN, 100.0, M).0, 1);
    }

    #[test]
    fn positions_follow_the_grid() {
        assert_eq!(x_of(10.0, 3, M), 34.0);
        assert_eq!(y_of(5.0, 2, M), 37.0);
    }

    #[test]
    fn wheel_up_scrolls_up_and_down_scrolls_down() {
        assert_eq!(scroll_lines(48.0, 16.0), -3);
        assert_eq!(scroll_lines(-48.0, 16.0), 3);
    }

    #[test]
    fn small_wheel_deltas_still_move_one_line() {
        assert_eq!(scroll_lines(2.0, 16.0), -1);
        assert_eq!(scroll_lines(-2.0, 16.0), 1);
        assert_eq!(scroll_lines(0.0, 16.0), 0);
        assert_eq!(scroll_lines(f32::NAN, 16.0), 0);
        assert_eq!(scroll_lines(10.0, 0.0), 0);
    }
}
