//! Interactive viewer state and key handling. Everything here is independent of drawing,
//! so it can be driven (and tested) with plain key events.
//!
//! Two kinds of keys:
//! - cursor keys (`j`/`k`, arrows, Tab, `g`/`G` for the first / last stop) move the cursor
//!   and the view follows it;
//! - view keys (space/`b`, PageUp/PageDown, `J`/`K`, Home/End) scroll the view and the
//!   cursor is pulled onto a stop that is still visible.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use engine::{Cursor, Document, EditKey, Form, Mode, Page, edit, scroll_into_view};
use std::fs;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

pub struct App {
    pub path: String,
    doc: Document,
    pub form: Form,
    pub page: Page,
    pub cursor: Cursor,
    pub top: usize,
    pub width: usize,
    /// Terminal height in rows; the last row is the status line.
    pub height: usize,
    pub notice: Option<String>,
    /// Last text copied or cut inside the app, used by `p` / Ctrl+V.
    pub register: String,
    /// Text the terminal should be asked to put on the system clipboard.
    pub pending_copy: Option<String>,
}

fn load(path: &str) -> Result<Document, String> {
    let src = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path, e))?;
    Document::parse(&src)
}

impl App {
    pub fn new(path: &str, width: usize, height: usize) -> Result<App, String> {
        let doc = load(path)?;
        Ok(App::from_document(path, doc, width, height))
    }

    pub fn from_document(path: &str, doc: Document, width: usize, height: usize) -> App {
        let width = width.max(1);
        let form = Form::default();
        // Layout errors (a bad style value, say) are reported before the terminal is touched
        // by `new`; here they fall back to an empty page with a notice.
        let (page, notice) = match doc.layout(width, &form) {
            Ok(p) => (p, None),
            Err(e) => (Page::default(), Some(e)),
        };
        let cursor = Cursor::new(Mode::Focused, &page);
        App {
            path: path.to_string(),
            doc,
            form,
            page,
            cursor,
            top: 0,
            width,
            height,
            notice,
            register: String::new(),
            pending_copy: None,
        }
    }

    pub fn view_h(&self) -> usize {
        self.height.saturating_sub(1).max(1)
    }

    fn max_top(&self) -> usize {
        self.page.rows.len().saturating_sub(self.view_h())
    }

    pub fn editing(&self) -> Option<usize> {
        edit::editing(&self.form)
    }

    /// The rows the view should keep visible: the caret row while editing, else the cursor.
    fn focus_range(&self) -> Option<Range<usize>> {
        if self.editing().is_some() {
            let caret_row = self
                .page
                .rows
                .iter()
                .position(|r| r.info.as_ref().is_some_and(|i| i.caret.is_some()));
            if let Some(i) = caret_row {
                return Some(i..i + 1);
            }
        }
        self.cursor.highlight(&self.page)
    }

    /// Scrolls just enough to show the cursor (or caret), then clamps.
    fn follow(&mut self) {
        if let Some(r) = self.focus_range() {
            self.top = scroll_into_view(self.top, self.view_h(), r);
        }
        self.top = self.top.min(self.max_top());
    }

    /// Lays the page out again after a resize, an edit or a mode change.
    fn relayout(&mut self) {
        match self.doc.layout(self.width, &self.form) {
            Ok(p) => {
                self.page = p;
                self.cursor.relayout(&self.page);
            }
            Err(e) => self.notice = Some(e),
        }
        self.follow();
    }

    pub fn on_resize(&mut self, cols: u16, rows: u16) {
        self.width = (cols as usize).max(1);
        self.height = rows as usize;
        self.relayout();
    }

    pub fn on_paste(&mut self, text: String) {
        self.notice = None;
        self.paste(text);
    }

    pub fn on_key(&mut self, key: KeyEvent) -> Flow {
        if key.kind == KeyEventKind::Release {
            return Flow::Continue;
        }
        self.notice = None;
        if self.editing().is_some() {
            self.key_editing(key);
            Flow::Continue
        } else {
            self.key_normal(key)
        }
    }

    // ── normal mode ─────────────────────────────────────────────────

    fn key_normal(&mut self, key: KeyEvent) -> Flow {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => return Flow::Quit,
                KeyCode::Char('v') => {
                    let text = self.register.clone();
                    self.paste(text);
                }
                _ => {}
            }
            return Flow::Continue;
        }
        let page = self.view_h() as isize;
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return Flow::Quit,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => self.step(true),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => self.step(false),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_by(page),
            KeyCode::PageUp | KeyCode::Char('b') => self.scroll_by(-page),
            KeyCode::Char('J') => self.scroll_by(1),
            KeyCode::Char('K') => self.scroll_by(-1),
            KeyCode::Char('g') => self.edge(false),
            KeyCode::Char('G') => self.edge(true),
            KeyCode::Home => self.scroll_to(0, false),
            KeyCode::End => self.scroll_to(self.max_top(), true),
            KeyCode::Char('m') => {
                let next = self.cursor.mode().toggle();
                self.cursor.set_mode(next, &self.page);
                self.follow();
                self.notice = Some(format!("cursor mode: {}", next.name()));
            }
            KeyCode::Char('y') | KeyCode::Char('c') => self.copy(),
            KeyCode::Char('p') => {
                let text = self.register.clone();
                self.paste(text);
            }
            KeyCode::Enter | KeyCode::Char('i') => self.start_edit(),
            KeyCode::Char('r') => self.reload(),
            _ => {}
        }
        Flow::Continue
    }

    /// Moves the cursor one stop; with nothing to rest on, scrolls one line instead.
    fn step(&mut self, down: bool) {
        if self.cursor.row().is_none() {
            self.scroll_by(if down { 1 } else { -1 });
            return;
        }
        let moved = if down {
            self.cursor.next(&self.page)
        } else {
            self.cursor.prev(&self.page)
        };
        if moved {
            self.follow();
        }
    }

    /// Moves the cursor to the first or last stop; with no stops, scrolls to that end.
    fn edge(&mut self, last: bool) {
        if self.cursor.row().is_none() {
            let top = if last { self.max_top() } else { 0 };
            self.scroll_to(top, last);
            return;
        }
        if last {
            self.cursor.last(&self.page);
        } else {
            self.cursor.first(&self.page);
        }
        self.follow();
    }

    fn scroll_by(&mut self, delta: isize) {
        let top = (self.top as isize + delta).clamp(0, self.max_top() as isize);
        self.top = top as usize;
        self.pull_cursor(delta > 0);
    }

    fn scroll_to(&mut self, top: usize, toward_end: bool) {
        self.top = top.min(self.max_top());
        self.pull_cursor(toward_end);
    }

    /// After the view moved, keeps the cursor on a visible stop when there is one.
    fn pull_cursor(&mut self, toward_end: bool) {
        let view = self.top..self.top + self.view_h();
        let Some(h) = self.cursor.highlight(&self.page) else {
            return;
        };
        if h.start < view.end && h.end > view.start {
            return;
        }
        self.cursor.place_in(&self.page, view, toward_end);
    }

    fn copy(&mut self) {
        match self.cursor.copy_text(&self.page) {
            Some(t) => self.set_copied(t),
            None => self.notice = Some("nothing to copy here".to_string()),
        }
    }

    fn set_copied(&mut self, text: String) {
        self.notice = Some(format!("copied {} chars", text.chars().count()));
        self.register = text.clone();
        self.pending_copy = Some(text);
    }

    fn start_edit(&mut self) {
        match self.cursor.input(&self.page) {
            Some(id) => {
                edit::begin(&mut self.form, &self.page, id);
                self.relayout();
            }
            None => self.notice = Some("not an input (enter edits inputs)".to_string()),
        }
    }

    fn reload(&mut self) {
        match load(&self.path) {
            Ok(doc) => {
                self.doc = doc;
                // Input numbers can shift between versions of the file, so edits are dropped.
                self.form = Form::default();
                self.relayout();
                self.notice = Some("reloaded".to_string());
            }
            Err(e) => self.notice = Some(e),
        }
    }

    // ── editing and paste ───────────────────────────────────────────

    fn key_editing(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => self.copy_value(),
                KeyCode::Char('v') => {
                    let text = self.register.clone();
                    self.paste(text);
                }
                KeyCode::Char('a') => self.edit_key(EditKey::Home),
                KeyCode::Char('e') => self.edit_key(EditKey::End),
                _ => {}
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Enter => self.stop_edit(),
            KeyCode::Tab => {
                self.stop_edit();
                self.step(true);
            }
            KeyCode::BackTab => {
                self.stop_edit();
                self.step(false);
            }
            KeyCode::Char(c) => self.edit_key(EditKey::Insert(c)),
            KeyCode::Backspace => self.edit_key(EditKey::Backspace),
            KeyCode::Delete => self.edit_key(EditKey::Delete),
            KeyCode::Left => self.edit_key(EditKey::Left),
            KeyCode::Right => self.edit_key(EditKey::Right),
            KeyCode::Home => self.edit_key(EditKey::Home),
            KeyCode::End => self.edit_key(EditKey::End),
            KeyCode::Up => self.edit_key(EditKey::Up),
            KeyCode::Down => self.edit_key(EditKey::Down),
            _ => {}
        }
    }

    fn edit_key(&mut self, k: EditKey) {
        edit::apply(&mut self.form, &self.page, k);
        self.relayout();
    }

    fn stop_edit(&mut self) {
        edit::end(&mut self.form);
        self.relayout();
    }

    /// Copies the value of the input being edited (also when it is protected).
    fn copy_value(&mut self) {
        let Some(id) = self.editing() else {
            return;
        };
        match edit::value(&self.form, &self.page, id) {
            Some(v) if !v.is_empty() => self.set_copied(v),
            _ => self.notice = Some("nothing to copy here".to_string()),
        }
    }

    /// Pastes at the caret. Outside editing, an input under the cursor is entered first.
    fn paste(&mut self, text: String) {
        if text.is_empty() {
            self.notice = Some("nothing to paste".to_string());
            return;
        }
        if self.editing().is_none() {
            match self.cursor.input(&self.page) {
                Some(id) => {
                    edit::begin(&mut self.form, &self.page, id);
                }
                None => {
                    self.notice = Some("move the cursor onto an input to paste".to_string());
                    return;
                }
            }
        }
        self.edit_key(EditKey::Paste(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "h1 \"Title\"\n\
        p \"plain text\"\n\
        link https://x.y \"Home\"\n\
        input \"name\" \"John\"\n\
        input.protected \"pw\" \"hunter2\"\n\
        button \"Go\"\n";

    fn app(src: &str, w: usize, h: usize) -> App {
        App::from_document("test.cre", Document::parse(src).unwrap(), w, h)
    }

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn ch(c: char) -> KeyEvent {
        key(KeyCode::Char(c))
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn row_of(app: &App) -> usize {
        app.cursor.row().unwrap()
    }

    fn text_of(app: &App, row: usize) -> String {
        engine::row_text(&app.page.rows[row]).trim_end().to_string()
    }

    #[test]
    fn starts_in_focused_mode_on_the_first_link() {
        let a = app(SRC, 40, 20);
        assert_eq!(a.cursor.mode(), Mode::Focused);
        assert_eq!(row_of(&a), 2);
    }

    #[test]
    fn j_and_k_move_between_focusables() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('j'));
        assert_eq!(row_of(&a), 3);
        a.on_key(key(KeyCode::Down));
        a.on_key(key(KeyCode::Tab));
        assert_eq!(row_of(&a), 5);
        assert_eq!(a.on_key(ch('j')), Flow::Continue);
        assert_eq!(row_of(&a), 5);
        a.on_key(key(KeyCode::BackTab));
        a.on_key(ch('k'));
        a.on_key(key(KeyCode::Up));
        assert_eq!(row_of(&a), 2);
    }

    #[test]
    fn m_switches_to_all_mode_and_back() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('m'));
        assert_eq!(a.cursor.mode(), Mode::All);
        assert_eq!(a.notice.as_deref(), Some("cursor mode: all"));
        a.on_key(ch('g'));
        assert_eq!(row_of(&a), 0);
        a.on_key(ch('j'));
        assert_eq!(row_of(&a), 1);
        a.on_key(ch('m'));
        assert_eq!(a.cursor.mode(), Mode::Focused);
        assert_eq!(row_of(&a), 2); // snapped forward from the plain text row
    }

    #[test]
    fn y_copies_the_line_under_the_cursor() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('y'));
        assert_eq!(a.pending_copy.take().as_deref(), Some("Home"));
        assert_eq!(a.register, "Home");
        assert_eq!(a.notice.as_deref(), Some("copied 4 chars"));
        a.on_key(ch('m'));
        a.on_key(ch('g'));
        a.on_key(ch('j'));
        a.on_key(ch('c'));
        assert_eq!(a.pending_copy.take().as_deref(), Some("plain text"));
    }

    #[test]
    fn copying_an_input_yields_its_value_even_when_protected() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('y'));
        assert_eq!(a.pending_copy.take().as_deref(), Some("John"));
        a.on_key(ch('j'));
        a.on_key(ch('y'));
        assert_eq!(a.pending_copy.take().as_deref(), Some("hunter2"));
    }

    #[test]
    fn copy_on_an_empty_page_reports_nothing_to_copy() {
        let mut a = app("p \"x\"", 10, 5);
        assert_eq!(a.cursor.row(), None);
        a.on_key(ch('y'));
        assert_eq!(a.pending_copy, None);
        assert_eq!(a.notice.as_deref(), Some("nothing to copy here"));
    }

    #[test]
    fn enter_edits_an_input_and_typing_changes_it() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('j'));
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.editing(), Some(0));
        a.on_key(ch('!'));
        assert_eq!(text_of(&a, 3), "name: [John!]");
        a.on_key(key(KeyCode::Left));
        a.on_key(key(KeyCode::Backspace));
        assert_eq!(text_of(&a, 3), "name: [Joh!]");
        a.on_key(key(KeyCode::Esc));
        assert_eq!(a.editing(), None);
        assert_eq!(text_of(&a, 3), "name: [Joh!]");
        // The edit survives resizes.
        a.on_resize(60, 20);
        assert_eq!(text_of(&a, 3), "name: [Joh!]");
    }

    #[test]
    fn typed_letters_do_not_trigger_commands_while_editing() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('i'));
        for c in ['q', 'm', 'y', 'j'] {
            assert_eq!(a.on_key(ch(c)), Flow::Continue);
        }
        assert_eq!(text_of(&a, 3), "name: [Johnqmyj]");
        assert_eq!(a.cursor.mode(), Mode::Focused);
        assert_eq!(a.pending_copy, None);
    }

    #[test]
    fn enter_on_a_link_does_not_edit() {
        let mut a = app(SRC, 40, 20);
        a.on_key(key(KeyCode::Enter));
        assert_eq!(a.editing(), None);
        assert_eq!(a.notice.as_deref(), Some("not an input (enter edits inputs)"));
    }

    #[test]
    fn ctrl_c_copies_while_editing_and_quits_otherwise() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('i'));
        a.on_key(ch('x'));
        assert_eq!(a.on_key(ctrl('c')), Flow::Continue);
        assert_eq!(a.pending_copy.take().as_deref(), Some("Johnx"));
        a.on_key(key(KeyCode::Esc));
        assert_eq!(a.on_key(ctrl('c')), Flow::Quit);
    }

    #[test]
    fn copy_then_paste_round_trips_through_the_register() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('y')); // copies "Home"
        a.on_key(ch('j')); // onto the name input
        a.on_key(ch('p')); // paste: enters the input and inserts at the end
        assert_eq!(a.editing(), Some(0));
        assert_eq!(text_of(&a, 3), "name: [JohnHome]");
        a.on_key(ctrl('v'));
        assert_eq!(text_of(&a, 3), "name: [JohnHomeHome]");
    }

    #[test]
    fn terminal_paste_inserts_at_the_caret() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('i'));
        a.on_key(key(KeyCode::Home));
        a.on_paste("Dr. ".to_string());
        assert_eq!(text_of(&a, 3), "name: [Dr. John]");
        // Line breaks from the clipboard become spaces.
        a.on_paste("a\nb".to_string());
        assert_eq!(text_of(&a, 3), "name: [Dr. a bJohn]");
    }

    #[test]
    fn terminal_paste_on_an_input_enters_it() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('j'));
        a.on_paste("X".to_string());
        assert_eq!(a.editing(), Some(0));
        assert_eq!(text_of(&a, 3), "name: [JohnX]");
    }

    #[test]
    fn paste_is_refused_off_an_input() {
        let mut a = app(SRC, 40, 20);
        a.on_paste("X".to_string());
        assert_eq!(a.editing(), None);
        assert_eq!(a.notice.as_deref(), Some("move the cursor onto an input to paste"));
        a.on_key(ch('p')); // empty register
        assert_eq!(a.notice.as_deref(), Some("nothing to paste"));
    }

    #[test]
    fn tab_leaves_the_input_and_moves_on() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('i'));
        a.on_key(key(KeyCode::Tab));
        assert_eq!(a.editing(), None);
        assert_eq!(row_of(&a), 4);
    }

    #[test]
    fn release_events_are_ignored() {
        let mut a = app(SRC, 40, 20);
        let mut k = ch('j');
        k.kind = KeyEventKind::Release;
        a.on_key(k);
        assert_eq!(row_of(&a), 2);
    }

    #[test]
    fn q_and_esc_quit() {
        let mut a = app(SRC, 40, 20);
        assert_eq!(a.on_key(ch('q')), Flow::Quit);
        let mut a = app(SRC, 40, 20);
        assert_eq!(a.on_key(key(KeyCode::Esc)), Flow::Quit);
    }

    fn long_page() -> String {
        let mut s = String::from("button \"first\"\n");
        for i in 0..40 {
            s.push_str(&format!("p \"line {}\"\n", i));
        }
        s.push_str("button \"last\"\n");
        s
    }

    #[test]
    fn view_keys_scroll_and_pull_the_cursor_onto_a_visible_stop() {
        let mut a = app(&long_page(), 30, 11); // 10 visible rows, 42 rows in total
        assert_eq!(row_of(&a), 0);
        a.on_key(ch(' '));
        assert_eq!(a.top, 10);
        // The first button scrolled out and no stop is visible, so the cursor stays.
        assert_eq!(row_of(&a), 0);
        a.on_key(key(KeyCode::End));
        assert_eq!(a.top, 32);
        assert_eq!(row_of(&a), 41); // the last button is visible and gets the cursor
        a.on_key(key(KeyCode::Home));
        assert_eq!(a.top, 0);
        assert_eq!(row_of(&a), 0);
        a.on_key(key(KeyCode::PageDown));
        a.on_key(key(KeyCode::PageUp));
        assert_eq!(a.top, 0);
    }

    #[test]
    fn g_and_capital_g_jump_the_cursor_to_the_first_and_last_stop() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('G'));
        assert_eq!(row_of(&a), 5);
        a.on_key(ch('g'));
        assert_eq!(row_of(&a), 2);
        // With text after the last stop, G goes to the stop and the view follows it.
        let mut b = app(&format!("{}p \"tail\"\n", long_page()), 30, 11);
        b.on_key(ch('G'));
        assert_eq!(row_of(&b), 41);
        assert!(b.top <= 41 && 41 < b.top + 10);
        // End scrolls to the very end of the page instead.
        b.on_key(key(KeyCode::End));
        assert_eq!(b.top, b.page.rows.len() - 10);
    }

    #[test]
    fn g_and_capital_g_scroll_when_there_are_no_stops() {
        let mut a = app("p \"a\"\np \"b\"\np \"c\"\np \"d\"", 10, 3);
        a.on_key(ch('G'));
        assert_eq!(a.top, 2);
        a.on_key(ch('g'));
        assert_eq!(a.top, 0);
    }

    #[test]
    fn j_and_k_without_stops_scroll_the_view() {
        let mut a = app("p \"a\"\np \"b\"\np \"c\"\np \"d\"", 10, 3); // 2 visible rows
        assert_eq!(a.cursor.row(), None);
        a.on_key(ch('j'));
        assert_eq!(a.top, 1);
        a.on_key(ch('j'));
        a.on_key(ch('j'));
        assert_eq!(a.top, 2); // clamped at the last full page
        a.on_key(ch('k'));
        assert_eq!(a.top, 1);
    }

    #[test]
    fn cursor_keys_scroll_the_view_to_follow() {
        let mut a = app(&long_page(), 30, 11);
        a.on_key(ch('j')); // first -> last button
        assert_eq!(row_of(&a), 41);
        assert_eq!(a.top, 32);
        a.on_key(ch('k'));
        assert_eq!(row_of(&a), 0);
        assert_eq!(a.top, 0);
    }

    #[test]
    fn all_mode_walks_every_line_and_scrolls() {
        let mut a = app(&long_page(), 30, 6); // 5 visible rows
        a.on_key(ch('m'));
        for _ in 0..7 {
            a.on_key(ch('j'));
        }
        assert_eq!(row_of(&a), 7);
        assert!(a.top <= 7 && 7 < a.top + 5);
        a.on_key(ch('y'));
        assert_eq!(a.pending_copy.take().as_deref(), Some("line 6"));
    }

    #[test]
    fn editing_keeps_the_caret_row_visible() {
        let src = "input.size:10 \"name\" \"hello world foo bar baz\"\n";
        let mut a = app(src, 10, 3); // 2 visible rows
        a.on_key(ch('i'));
        a.on_key(ch('!'));
        let caret_row = a
            .page
            .rows
            .iter()
            .position(|r| r.info.as_ref().unwrap().caret.is_some())
            .unwrap();
        assert!(a.top <= caret_row && caret_row < a.top + 2);
    }

    #[test]
    fn resize_relayouts_and_keeps_the_cursor_on_its_element() {
        let mut a = app(SRC, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('j'));
        assert_eq!(a.cursor.current(&a.page).unwrap().input, Some(1));
        a.on_resize(8, 20);
        assert_eq!(a.cursor.current(&a.page).unwrap().input, Some(1));
        assert!(a.page.rows.len() > 6);
    }

    #[test]
    fn a_page_that_fails_layout_shows_the_error_instead_of_crashing() {
        let a = app("p.size:wide \"x\"", 10, 5);
        assert!(a.notice.as_deref().unwrap().contains("not a valid size"));
        assert!(a.page.rows.is_empty());
    }

    #[test]
    fn reload_picks_up_changes_and_drops_edits() {
        let dir = std::env::temp_dir().join(format!("brewster-app-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("page.cre");
        fs::write(&path, "input \"a\" \"one\"\n").unwrap();
        let mut a = App::new(path.to_str().unwrap(), 30, 10).unwrap();
        a.on_key(ch('i'));
        a.on_key(ch('X'));
        a.on_key(key(KeyCode::Esc));
        assert_eq!(text_of(&a, 0), "a: [oneX]");
        fs::write(&path, "input \"a\" \"two\"\n").unwrap();
        a.on_key(ch('r'));
        assert_eq!(text_of(&a, 0), "a: [two]");
        assert_eq!(a.notice.as_deref(), Some("reloaded"));
        // A broken file keeps the old page and reports the error.
        fs::write(&path, "blink \"x\"\n").unwrap();
        a.on_key(ch('r'));
        assert_eq!(text_of(&a, 0), "a: [two]");
        assert!(a.notice.as_deref().unwrap().contains("unknown element"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_a_normal_error() {
        let e = App::new("/definitely/not/here.cre", 10, 5).err().unwrap();
        assert!(e.starts_with("cannot read"));
    }
}
