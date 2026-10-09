//! Cursor: where the reader is on a laid-out page, and what they can do there.
//!
//! The cursor rests on rows of a [`Page`] and has two modes:
//! - [`Mode::Focused`] grabs onto links, inputs and buttons only. A link, button or
//!   input that wraps over several rows is a single stop.
//! - [`Mode::All`] grabs onto every line that shows element text (rows of pure
//!   margin, border or padding are skipped).
//!
//! From the cursor an app can read the text to copy ([`Cursor::copy_text`]) and find
//! the input to edit ([`Cursor::input`], then [`edit`]). No terminal or clipboard
//! access happens here; apps do that.

pub mod edit;

use layout::{LineInfo, Page, Row};
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Links, inputs and buttons.
    #[default]
    Focused,
    /// Every line of text.
    All,
}

impl Mode {
    pub fn toggle(self) -> Mode {
        match self {
            Mode::Focused => Mode::All,
            Mode::All => Mode::Focused,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Focused => "focused",
            Mode::All => "all",
        }
    }
}

fn is_target(mode: Mode, row: &Row) -> bool {
    match (&row.info, mode) {
        (None, _) => false,
        (Some(_), Mode::All) => true,
        (Some(i), Mode::Focused) => i.focus.is_some(),
    }
}

fn focus_of(row: &Row) -> Option<usize> {
    row.info.as_ref().and_then(|i| i.focus)
}

#[derive(Debug, Clone, Default)]
pub struct Cursor {
    mode: Mode,
    /// Index into `page.rows`; `None` when the page has nothing to rest on.
    row: Option<usize>,
    /// Focus id of the element under the cursor, used to find it again after a relayout.
    focus: Option<usize>,
}

impl Cursor {
    pub fn new(mode: Mode, page: &Page) -> Cursor {
        let mut c = Cursor {
            mode,
            row: None,
            focus: None,
        };
        c.snap(page, 0);
        c
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn row(&self) -> Option<usize> {
        self.row
    }

    fn set(&mut self, page: &Page, row: Option<usize>) {
        self.row = row;
        self.focus = row.and_then(|r| page.rows.get(r)).and_then(focus_of);
    }

    fn is_target_at(&self, page: &Page, i: usize) -> bool {
        page.rows.get(i).is_some_and(|r| is_target(self.mode, r))
    }

    /// True when row `i` belongs to the element the cursor is on and the mode treats
    /// that element as one stop.
    fn same_stop(&self, page: &Page, i: usize) -> bool {
        self.mode == Mode::Focused
            && self.focus.is_some()
            && page.rows.get(i).and_then(focus_of) == self.focus
    }

    /// First target at or after `from`, else the last one before it.
    fn snap(&mut self, page: &Page, from: usize) {
        let len = page.rows.len();
        let found = (from..len)
            .find(|&i| self.is_target_at(page, i))
            .or_else(|| {
                (0..from.min(len))
                    .rev()
                    .find(|&i| self.is_target_at(page, i))
            });
        let found = found.map(|r| self.start_of_element(page, r));
        self.set(page, found);
    }

    /// In focused mode, the first row of the element that row `r` belongs to.
    fn start_of_element(&self, page: &Page, r: usize) -> usize {
        if self.mode != Mode::Focused {
            return r;
        }
        let Some(f) = page.rows.get(r).and_then(focus_of) else {
            return r;
        };
        let mut s = r;
        while s > 0 && focus_of(&page.rows[s - 1]) == Some(f) {
            s -= 1;
        }
        s
    }

    /// Switches mode, staying put when the current row is valid in the new mode and
    /// otherwise moving to the nearest row that is.
    pub fn set_mode(&mut self, mode: Mode, page: &Page) {
        self.mode = mode;
        let from = self.row.unwrap_or(0);
        self.snap(page, from);
    }

    /// Re-anchors after the page was laid out again (resize, edit). The element under
    /// the cursor is found by its focus id; on plain text the row index is kept.
    pub fn relayout(&mut self, page: &Page) {
        let by_focus = self.focus.and_then(|f| {
            page.rows
                .iter()
                .position(|r| focus_of(r) == Some(f))
        });
        match by_focus {
            Some(i) => self.set(page, Some(i)),
            None => {
                let from = self.row.unwrap_or(0);
                self.snap(page, from);
            }
        }
    }

    /// Moves to the next stop. Returns false at the end of the page.
    pub fn next(&mut self, page: &Page) -> bool {
        let Some(cur) = self.row else {
            return false;
        };
        let found = (cur + 1..page.rows.len())
            .find(|&i| self.is_target_at(page, i) && !self.same_stop(page, i));
        match found {
            Some(i) => {
                self.set(page, Some(i));
                true
            }
            None => false,
        }
    }

    /// Moves to the previous stop. Returns false at the start of the page.
    pub fn prev(&mut self, page: &Page) -> bool {
        let Some(cur) = self.row else {
            return false;
        };
        let found = (0..cur)
            .rev()
            .find(|&i| self.is_target_at(page, i) && !self.same_stop(page, i));
        match found {
            Some(i) => {
                let i = self.start_of_element(page, i);
                self.set(page, Some(i));
                true
            }
            None => false,
        }
    }

    pub fn first(&mut self, page: &Page) {
        self.snap(page, 0);
    }

    pub fn last(&mut self, page: &Page) {
        let last = (0..page.rows.len())
            .rev()
            .find(|&i| self.is_target_at(page, i))
            .map(|r| self.start_of_element(page, r));
        if last.is_some() {
            self.set(page, last);
        }
    }

    /// Moves roughly `delta` rows (for paging): to the first stop at or beyond the
    /// target row in that direction, or to the end of the page when there is none.
    pub fn jump(&mut self, page: &Page, delta: isize) {
        let len = page.rows.len();
        if len == 0 {
            return;
        }
        let cur = self.row.unwrap_or(0) as isize;
        let t = (cur + delta).clamp(0, len as isize - 1) as usize;
        if delta >= 0 {
            match (t..len).find(|&i| self.is_target_at(page, i) && !self.same_stop(page, i)) {
                Some(i) => self.set(page, Some(i)),
                None => self.last(page),
            }
        } else {
            match (0..=t)
                .rev()
                .find(|&i| self.is_target_at(page, i) && !self.same_stop(page, i))
            {
                Some(i) => {
                    let i = self.start_of_element(page, i);
                    self.set(page, Some(i));
                }
                None => self.first(page),
            }
        }
    }

    /// Rests the cursor on the first (or, with `last`, the last) stop whose row lies in
    /// `rows`. Returns false, leaving the cursor where it is, when there is none.
    pub fn place_in(&mut self, page: &Page, rows: Range<usize>, last: bool) -> bool {
        let end = rows.end.min(page.rows.len());
        let mut range = rows.start..end;
        let found = if last {
            range.rev().find(|&i| self.is_target_at(page, i))
        } else {
            range.find(|&i| self.is_target_at(page, i))
        };
        match found {
            Some(i) => {
                self.set(page, Some(i));
                true
            }
            None => false,
        }
    }

    /// Rows to highlight: every row of the element in focused mode, one line in all mode.
    pub fn highlight(&self, page: &Page) -> Option<Range<usize>> {
        let r = self.row?;
        page.rows.get(r)?;
        if self.mode == Mode::Focused {
            if let Some(f) = self.focus {
                let mut s = r;
                while s > 0 && focus_of(&page.rows[s - 1]) == Some(f) {
                    s -= 1;
                }
                let mut e = r + 1;
                while e < page.rows.len() && focus_of(&page.rows[e]) == Some(f) {
                    e += 1;
                }
                return Some(s..e);
            }
        }
        Some(r..r + 1)
    }

    pub fn current<'a>(&self, page: &'a Page) -> Option<&'a LineInfo> {
        page.rows.get(self.row?)?.info.as_ref()
    }

    /// The text "copy" yields at the cursor: the visual line for text and `pre`, the whole
    /// text for a link, the label for a button, the value for an input (also when it is
    /// protected). `None` when there is nothing to copy.
    pub fn copy_text(&self, page: &Page) -> Option<String> {
        let t = &self.current(page)?.text;
        if t.is_empty() {
            None
        } else {
            Some(t.clone())
        }
    }

    /// Index of the input under the cursor, if the cursor is on one.
    pub fn input(&self, page: &Page) -> Option<usize> {
        self.current(page)?.input
    }
}

/// New top row for a view `view_h` rows tall so that `range` is visible. A range taller
/// than the view shows its start.
pub fn scroll_into_view(top: usize, view_h: usize, range: Range<usize>) -> usize {
    let view_h = view_h.max(1);
    let mut top = top;
    if range.end > top + view_h {
        top = range.end - view_h;
    }
    if range.start < top {
        top = range.start;
    }
    top
}

#[cfg(test)]
pub(crate) mod fixtures {
    use layout::{Form, Page, layout_all};
    use resolver::{Resolved, Style};

    pub fn el(
        tag: &str,
        style: &[(&str, &[&str])],
        args: &[&str],
        children: Vec<Resolved>,
    ) -> Resolved {
        let mut s = Style::new();
        for (k, v) in style {
            s.insert(k.to_string(), v.iter().map(|x| x.to_string()).collect());
        }
        Resolved {
            tag: tag.to_string(),
            style: s,
            args: args.iter().map(|x| x.to_string()).collect(),
            children,
        }
    }

    pub fn page(roots: &[Resolved], width: usize) -> Page {
        layout_all(roots, width, &Form::default()).unwrap()
    }

    /// rows: 0 title, 1 text, 2 link (focus 0), 3 input (focus 1), 4 button (focus 2)
    pub fn sample() -> Page {
        page(
            &[
                el("h1", &[], &["Title"], vec![]),
                el("p", &[], &["plain text"], vec![]),
                el("link", &[], &["https://x.y", "Home"], vec![]),
                el("input", &[], &["name", "John"], vec![]),
                el("button", &[], &["Go"], vec![]),
            ],
            30,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use layout::Kind;

    #[test]
    fn focused_mode_starts_on_the_first_focusable_row() {
        let p = sample();
        let c = Cursor::new(Mode::Focused, &p);
        assert_eq!(c.row(), Some(2));
        assert_eq!(c.current(&p).unwrap().kind, Kind::Link);
    }

    #[test]
    fn all_mode_starts_on_the_first_line() {
        let p = sample();
        assert_eq!(Cursor::new(Mode::All, &p).row(), Some(0));
    }

    #[test]
    fn focused_mode_moves_between_focusables_only() {
        let p = sample();
        let mut c = Cursor::new(Mode::Focused, &p);
        assert!(c.next(&p));
        assert_eq!(c.row(), Some(3));
        assert!(c.next(&p));
        assert_eq!(c.row(), Some(4));
        assert!(!c.next(&p));
        assert_eq!(c.row(), Some(4));
        assert!(c.prev(&p));
        assert!(c.prev(&p));
        assert_eq!(c.row(), Some(2));
        assert!(!c.prev(&p));
    }

    #[test]
    fn all_mode_visits_every_line() {
        let p = sample();
        let mut c = Cursor::new(Mode::All, &p);
        let mut seen = vec![c.row().unwrap()];
        while c.next(&p) {
            seen.push(c.row().unwrap());
        }
        assert_eq!(seen, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn all_mode_skips_margin_border_and_padding_rows() {
        let p = page(
            &[el(
                "p",
                &[("margin", &["1"]), ("pad", &["1"]), ("border", &["1"])],
                &["x"],
                vec![],
            )],
            10,
        );
        assert_eq!(p.rows.len(), 7);
        let mut c = Cursor::new(Mode::All, &p);
        assert_eq!(c.row(), Some(3));
        assert!(!c.next(&p));
        assert!(!c.prev(&p));
    }

    #[test]
    fn switching_modes_keeps_or_snaps_the_position() {
        let p = sample();
        let mut c = Cursor::new(Mode::All, &p);
        c.next(&p); // row 1, plain text
        c.set_mode(Mode::Focused, &p);
        assert_eq!(c.row(), Some(2)); // snapped forward to the link
        c.next(&p);
        c.set_mode(Mode::All, &p);
        assert_eq!(c.row(), Some(3)); // stays on the input
        assert_eq!(c.mode(), Mode::All);
    }

    #[test]
    fn snapping_falls_back_to_the_previous_stop_at_the_end() {
        // Trailing plain text after the last focusable.
        let p = page(
            &[
                el("button", &[], &["B"], vec![]),
                el("p", &[], &["tail"], vec![]),
            ],
            10,
        );
        let mut c = Cursor::new(Mode::All, &p);
        c.next(&p);
        assert_eq!(c.row(), Some(1));
        c.set_mode(Mode::Focused, &p);
        assert_eq!(c.row(), Some(0));
    }

    fn wrapped() -> Page {
        // Rows: 0-1 link "aaa bbb ccc" (wraps at 7), 2 button, 3 text
        page(
            &[
                el("link", &[], &["u", "aaa bbb ccc"], vec![]),
                el("button", &[], &["B"], vec![]),
                el("p", &[], &["t"], vec![]),
            ],
            7,
        )
    }

    #[test]
    fn a_wrapped_element_is_one_stop_in_focused_mode() {
        let p = wrapped();
        assert_eq!(p.rows.len(), 4);
        let mut c = Cursor::new(Mode::Focused, &p);
        assert_eq!(c.row(), Some(0));
        assert_eq!(c.highlight(&p), Some(0..2));
        assert!(c.next(&p));
        assert_eq!(c.row(), Some(2));
        assert!(c.prev(&p));
        assert_eq!(c.row(), Some(0)); // lands on the first row of the link
        assert!(!c.prev(&p));
    }

    #[test]
    fn all_mode_steps_through_each_row_of_a_wrapped_element() {
        let p = wrapped();
        let mut c = Cursor::new(Mode::All, &p);
        assert_eq!(c.highlight(&p), Some(0..1));
        assert!(c.next(&p));
        assert_eq!(c.row(), Some(1));
        assert_eq!(c.highlight(&p), Some(1..2));
        // The second row of the link still copies the whole link.
        assert_eq!(c.copy_text(&p).as_deref(), Some("aaa bbb ccc"));
    }

    #[test]
    fn copy_yields_the_line_label_or_value() {
        let p = sample();
        let mut c = Cursor::new(Mode::All, &p);
        assert_eq!(c.copy_text(&p).as_deref(), Some("TITLE")); // header marks not copied
        c.next(&p);
        assert_eq!(c.copy_text(&p).as_deref(), Some("plain text"));
        c.next(&p);
        assert_eq!(c.copy_text(&p).as_deref(), Some("Home"));
        c.next(&p);
        assert_eq!(c.copy_text(&p).as_deref(), Some("John"));
        c.next(&p);
        assert_eq!(c.copy_text(&p).as_deref(), Some("Go"));
    }

    #[test]
    fn copying_a_protected_input_yields_its_value() {
        let p = page(&[el("input", &[("protected", &[])], &["pw", "hunter2"], vec![])], 30);
        let c = Cursor::new(Mode::Focused, &p);
        assert_eq!(c.copy_text(&p).as_deref(), Some("hunter2"));
    }

    #[test]
    fn nothing_to_copy_on_an_empty_line() {
        let p = page(&[el("p", &[], &[""], vec![])], 10);
        let c = Cursor::new(Mode::All, &p);
        assert_eq!(c.row(), Some(0));
        assert_eq!(c.copy_text(&p), None);
    }

    #[test]
    fn input_is_reported_only_on_inputs() {
        let p = sample();
        let mut c = Cursor::new(Mode::Focused, &p);
        assert_eq!(c.input(&p), None); // on the link
        c.next(&p);
        assert_eq!(c.input(&p), Some(0));
    }

    #[test]
    fn page_without_targets_has_no_cursor_in_focused_mode() {
        let p = page(&[el("p", &[], &["just text"], vec![])], 20);
        let mut c = Cursor::new(Mode::Focused, &p);
        assert_eq!(c.row(), None);
        assert!(!c.next(&p));
        assert!(!c.prev(&p));
        assert_eq!(c.copy_text(&p), None);
        assert_eq!(c.highlight(&p), None);
        c.set_mode(Mode::All, &p);
        assert_eq!(c.row(), Some(0));
    }

    #[test]
    fn empty_page_is_safe() {
        let p = Page::default();
        let mut c = Cursor::new(Mode::All, &p);
        assert_eq!(c.row(), None);
        c.next(&p);
        c.prev(&p);
        c.first(&p);
        c.last(&p);
        c.jump(&p, 5);
        c.relayout(&p);
        assert_eq!(c.row(), None);
    }

    #[test]
    fn first_and_last() {
        let p = sample();
        let mut c = Cursor::new(Mode::Focused, &p);
        c.last(&p);
        assert_eq!(c.row(), Some(4));
        c.first(&p);
        assert_eq!(c.row(), Some(2));
        let mut a = Cursor::new(Mode::All, &p);
        a.last(&p);
        assert_eq!(a.row(), Some(4));
    }

    #[test]
    fn jump_moves_by_rows_and_stops_at_the_ends() {
        let items: Vec<_> = (0..10).map(|_| el("p", &[], &["x"], vec![])).collect();
        let p = page(&items, 5);
        let mut c = Cursor::new(Mode::All, &p);
        c.jump(&p, 4);
        assert_eq!(c.row(), Some(4));
        c.jump(&p, 100);
        assert_eq!(c.row(), Some(9));
        c.jump(&p, -3);
        assert_eq!(c.row(), Some(6));
        c.jump(&p, -100);
        assert_eq!(c.row(), Some(0));
    }

    #[test]
    fn jump_never_gets_stuck_on_a_tall_element() {
        // One link taller than the jump, then a button.
        let p = page(
            &[
                el("link", &[], &["u", "a b c d e f g h"], vec![]),
                el("button", &[], &["B"], vec![]),
            ],
            3,
        );
        let mut c = Cursor::new(Mode::Focused, &p);
        assert_eq!(c.row(), Some(0));
        c.jump(&p, 2); // row 2 is still inside the link, so it moves on to the button
        let button_row = p
            .rows
            .iter()
            .position(|r| r.info.as_ref().unwrap().kind == Kind::Button);
        assert_eq!(c.row(), button_row);
        assert_eq!(c.current(&p).unwrap().kind, Kind::Button);
        c.jump(&p, -2);
        assert_eq!(c.row(), Some(0));
    }

    #[test]
    fn relayout_follows_the_element_by_focus_id() {
        let nodes = [
            el("p", &[], &["aaa bbb ccc ddd"], vec![]),
            el("link", &[], &["u", "L"], vec![]),
            el("button", &[], &["Go"], vec![]),
        ];
        let wide = page(&nodes, 30);
        let mut c = Cursor::new(Mode::Focused, &wide);
        c.next(&wide);
        assert_eq!(c.row(), Some(2)); // button, after one text row and the link
        let narrow = page(&nodes, 4);
        assert!(narrow.rows.len() > wide.rows.len());
        c.relayout(&narrow);
        assert_eq!(c.current(&narrow).unwrap().kind, Kind::Button);
        assert!(c.row().unwrap() > 2); // the wrapped text pushed the button down
    }

    #[test]
    fn relayout_clamps_when_the_row_is_gone() {
        let nodes: Vec<_> = (0..6).map(|_| el("p", &[], &["x"], vec![])).collect();
        let tall = page(&nodes, 5);
        let mut c = Cursor::new(Mode::All, &tall);
        c.last(&tall);
        assert_eq!(c.row(), Some(5));
        let short = page(&nodes[..2], 5);
        c.relayout(&short);
        assert_eq!(c.row(), Some(1));
    }

    #[test]
    fn place_in_picks_a_stop_inside_a_row_range() {
        let p = sample(); // rows: 0 title, 1 text, 2 link, 3 input, 4 button
        let mut c = Cursor::new(Mode::Focused, &p);
        assert!(c.place_in(&p, 3..5, false));
        assert_eq!(c.row(), Some(3));
        assert!(c.place_in(&p, 2..5, true));
        assert_eq!(c.row(), Some(4));
        // Rows 0 and 1 are plain text: no stop in focused mode, so the cursor stays.
        assert!(!c.place_in(&p, 0..2, false));
        assert_eq!(c.row(), Some(4));
        let mut a = Cursor::new(Mode::All, &p);
        assert!(a.place_in(&p, 0..2, true));
        assert_eq!(a.row(), Some(1));
        // Ranges past the end of the page are clamped.
        assert!(a.place_in(&p, 3..100, true));
        assert_eq!(a.row(), Some(4));
        assert!(!a.place_in(&p, 9..12, false));
    }

    #[test]
    fn highlight_in_all_mode_is_one_line() {
        let p = sample();
        let mut c = Cursor::new(Mode::All, &p);
        c.next(&p);
        assert_eq!(c.highlight(&p), Some(1..2));
    }

    #[test]
    fn mode_toggle_and_names() {
        assert_eq!(Mode::Focused.toggle(), Mode::All);
        assert_eq!(Mode::All.toggle(), Mode::Focused);
        assert_eq!(Mode::default(), Mode::Focused);
        assert_eq!(Mode::All.name(), "all");
    }

    #[test]
    fn scroll_keeps_the_range_visible() {
        assert_eq!(scroll_into_view(0, 10, 3..4), 0);
        assert_eq!(scroll_into_view(0, 10, 12..13), 3);
        assert_eq!(scroll_into_view(8, 10, 2..3), 2);
        // A range taller than the view shows its start.
        assert_eq!(scroll_into_view(0, 4, 10..30), 10);
        assert_eq!(scroll_into_view(50, 4, 10..30), 10);
        // Zero-height views are treated as one row.
        assert_eq!(scroll_into_view(0, 0, 5..6), 5);
    }
}
