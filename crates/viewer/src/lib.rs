//! Viewer: the state and key handling of an interactive page viewer, independent of any
//! terminal or window. A frontend (terminal, GUI) turns its own input events into
//! [`KeyPress`]es, calls [`Viewer::on_key`] / [`Viewer::on_paste`] / [`Viewer::on_resize`],
//! then paints [`Viewer::page`] from [`Viewer::top`] and sends [`Viewer::take_copy`] to
//! the system clipboard.
//!
//! Two kinds of keys:
//! - cursor keys (`j`/`k`, arrows, Tab, `g`/`G` for the first / last stop) move the cursor
//!   and the view follows it;
//! - view keys (space/`b`, PageUp/PageDown, `J`/`K`, Home/End) scroll the view and the
//!   cursor is pulled onto a stop that is still visible.

pub mod paint;

use std::ops::Range;

use engine::{
    Cursor, Document, EditKey, Form, Kind, LayoutOptions, Mode, Page, edit, scroll_into_view,
};

/// A key, as far as the viewer cares. Printable input is `Char`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Esc,
    Enter,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
    /// A function key, `F(1)` to `F(12)`.
    F(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    pub key: Key,
    pub mods: Mods,
}

impl KeyPress {
    pub fn new(key: Key, mods: Mods) -> KeyPress {
        KeyPress { key, mods }
    }

    /// A key without modifiers.
    pub fn plain(key: Key) -> KeyPress {
        KeyPress {
            key,
            mods: Mods::default(),
        }
    }

    /// A key with Ctrl held.
    pub fn ctrl(key: Key) -> KeyPress {
        KeyPress {
            key,
            mods: Mods {
                ctrl: true,
                alt: false,
                shift: false,
            },
        }
    }
}

/// Where a followed link opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Same,
    Tab,
    Window,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flow {
    Continue,
    /// A link was followed. `url` is as written in the page (not yet resolved).
    Open {
        url: String,
        target: Target,
    },
}

pub struct Viewer {
    /// What the page is called (its address); shown in the status line.
    pub path: String,
    doc: Document,
    /// The document is as high as the screen: top-level `size:max` fills it.
    pub fill: bool,
    /// Draw header font sizes (graphical frontends only).
    pub scale_headers: bool,
    /// Whether Enter on a link follows it (`Flow::Open`). The browser turns this off and
    /// follows links through its key bindings instead.
    pub enter_follows_links: bool,
    pub form: Form,
    pub page: Page,
    pub cursor: Cursor,
    pub top: usize,
    pub width: usize,
    /// Height in rows including one row for the status line (`view_h` is one less).
    pub height: usize,
    pub notice: Option<String>,
    /// Last text copied or cut inside the viewer, used by `p` / Ctrl+V.
    pub register: String,
    /// Text the frontend should put on the system clipboard (see [`Viewer::take_copy`]).
    pub pending_copy: Option<String>,
}

impl Viewer {
    pub fn from_document(path: &str, doc: Document, width: usize, height: usize) -> Viewer {
        let width = width.max(1);
        let form = Form::default();
        let opts = LayoutOptions {
            viewport_rows: Some(height.saturating_sub(1).max(1)),
            scale_headers: false,
        };
        // Layout errors (a bad style value, say) fall back to an empty page with a notice.
        let (page, notice) = match doc.layout(width, &form, opts) {
            Ok(p) => (p, None),
            Err(e) => (Page::default(), Some(e)),
        };
        let cursor = Cursor::new(Mode::Focused, &page);
        Viewer {
            path: path.to_string(),
            doc,
            fill: true,
            scale_headers: false,
            enter_follows_links: true,
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

    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// The layout options for the current screen.
    fn options(&self) -> LayoutOptions {
        LayoutOptions {
            viewport_rows: self.fill.then(|| self.view_h()),
            scale_headers: self.scale_headers,
        }
    }

    /// Changes how the page is fitted to the screen and draws it again.
    pub fn set_options(&mut self, fill: bool, scale_headers: bool) {
        self.fill = fill;
        self.scale_headers = scale_headers;
        self.relayout();
    }

    /// Sets the cursor mode (the browser applies the `view.cursor_mode` setting).
    pub fn set_mode(&mut self, mode: Mode) {
        self.cursor.set_mode(mode, &self.page);
        self.follow();
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

    /// Rows to draw as "under the cursor" (see [`paint::row_pieces`]).
    pub fn highlight(&self) -> Option<Range<usize>> {
        self.cursor.highlight(&self.page)
    }

    /// Takes the text a frontend should put on the system clipboard.
    pub fn take_copy(&mut self) -> Option<String> {
        self.pending_copy.take()
    }

    /// The line a frontend shows in its status bar.
    pub fn status(&self) -> String {
        if let Some(n) = &self.notice {
            return n.replace('\n', " ");
        }
        if let Some(id) = self.editing() {
            return format!(
                "editing input {}  type to edit  arrows move  ctrl-c copy  ctrl-v paste  enter/esc done",
                id + 1
            );
        }
        let pos = match self.cursor.row() {
            Some(r) => format!("line {}/{}", r + 1, self.page.rows.len()),
            None => "no cursor".to_string(),
        };
        format!(
            "{}  [{}] {}  q quit  m mode  j/k move  y copy  enter edit  space/b page",
            self.path,
            self.cursor.mode().name(),
            pos
        )
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
        match self.doc.layout(self.width, &self.form, self.options()) {
            Ok(p) => {
                self.page = p;
                self.cursor.relayout(&self.page);
            }
            Err(e) => self.notice = Some(e),
        }
        self.follow();
    }

    /// The view is now `cols` x `rows` cells, the status line row included in `rows`.
    pub fn on_resize(&mut self, cols: usize, rows: usize) {
        self.width = cols.max(1);
        self.height = rows;
        self.relayout();
    }

    pub fn on_paste(&mut self, text: String) {
        self.notice = None;
        self.paste(text);
    }

    /// Scrolls the view by `delta` lines (negative is up), as a mouse wheel does.
    pub fn scroll_lines(&mut self, delta: isize) {
        self.scroll_by(delta);
    }

    /// "Copy" as a frontend's copy command (Ctrl+C / Cmd+C / the copy event): the value of
    /// the input being edited, else the line under the cursor.
    pub fn copy_selection(&mut self) {
        self.notice = None;
        if self.editing().is_some() {
            self.copy_value();
        } else {
            self.copy();
        }
    }

    pub fn on_key(&mut self, key: KeyPress) -> Flow {
        self.notice = None;
        if self.editing().is_some() {
            self.key_editing(key);
            Flow::Continue
        } else {
            self.key_normal(key)
        }
    }

    // ── normal mode ─────────────────────────────────────────────────

    fn key_normal(&mut self, key: KeyPress) -> Flow {
        if key.mods.ctrl {
            match key.key {
                Key::Char('v') => {
                    let text = self.register.clone();
                    self.paste(text);
                }
                _ => {}
            }
            return Flow::Continue;
        }
        let page = self.view_h() as isize;
        match key.key {
            Key::Down | Key::Char('j') | Key::Tab => self.step(true),
            Key::Up | Key::Char('k') | Key::BackTab => self.step(false),
            Key::PageDown | Key::Char(' ') => self.scroll_by(page),
            Key::PageUp | Key::Char('b') => self.scroll_by(-page),
            Key::Char('J') => self.scroll_by(1),
            Key::Char('K') => self.scroll_by(-1),
            Key::Char('g') => self.edge(false),
            Key::Char('G') => self.edge(true),
            Key::Home => self.scroll_to(0, false),
            Key::End => self.scroll_to(self.max_top(), true),
            Key::Char('m') => {
                let next = self.cursor.mode().toggle();
                self.cursor.set_mode(next, &self.page);
                self.follow();
                self.notice = Some(format!("cursor mode: {}", next.name()));
            }
            Key::Char('y') | Key::Char('c') => self.copy(),
            Key::Char('p') => {
                let text = self.register.clone();
                self.paste(text);
            }
            Key::Enter => return self.activate(),
            Key::Char('i') => self.start_edit(),
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

    /// The address of the link under the cursor, as written in the page.
    pub fn current_href(&self) -> Option<String> {
        let info = self.cursor.current(&self.page)?;
        if info.kind == Kind::Link {
            info.href.clone()
        } else {
            None
        }
    }

    /// Enter on the cursor: runs a button's `onclick`, follows a link (after its `onclick`,
    /// if it has one), or starts editing an input.
    pub fn activate(&mut self) -> Flow {
        let Some(info) = self.cursor.current(&self.page).cloned() else {
            self.notice = Some("nothing under the cursor".to_string());
            return Flow::Continue;
        };
        let mut ran = false;
        if self.doc.is_clickable(info.node) {
            ran = true;
            match self.doc.click(info.node) {
                Ok(_) => {}
                Err(e) => self.notice = Some(e),
            }
            // The tree may have changed (even on error, up to the failing statement).
            self.relayout();
        }
        match info.kind {
            Kind::Link => match info.href {
                Some(url) if self.enter_follows_links => Flow::Open {
                    url,
                    target: Target::Same,
                },
                _ => Flow::Continue,
            },
            Kind::Input => {
                if info.input.is_some() {
                    self.start_edit();
                }
                Flow::Continue
            }
            _ => {
                if !ran {
                    self.notice =
                        Some("nothing to do here (no link, input or onclick)".to_string());
                }
                Flow::Continue
            }
        }
    }

    /// Moves the cursor to the stop on page row `row` (a mouse press). False when that row
    /// has nothing the cursor can rest on in the current mode.
    pub fn select_row(&mut self, row: usize) -> bool {
        if self.editing().is_some() {
            self.stop_edit();
        }
        if row >= self.page.rows.len() {
            return false;
        }
        let ok = self.cursor.place_in(&self.page, row..row + 1, false);
        if ok {
            self.follow();
        }
        ok
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

    // ── editing and paste ───────────────────────────────────────────

    fn key_editing(&mut self, key: KeyPress) {
        if key.mods.ctrl {
            match key.key {
                Key::Char('c') => self.copy_value(),
                Key::Char('v') => {
                    let text = self.register.clone();
                    self.paste(text);
                }
                Key::Char('a') => self.edit_key(EditKey::Home),
                Key::Char('e') => self.edit_key(EditKey::End),
                _ => {}
            }
            return;
        }
        if key.mods.alt {
            return;
        }
        match key.key {
            Key::Esc | Key::Enter => self.stop_edit(),
            Key::Tab => {
                self.stop_edit();
                self.step(true);
            }
            Key::BackTab => {
                self.stop_edit();
                self.step(false);
            }
            Key::Char(c) => self.edit_key(EditKey::Insert(c)),
            Key::Backspace => self.edit_key(EditKey::Backspace),
            Key::Delete => self.edit_key(EditKey::Delete),
            Key::Left => self.edit_key(EditKey::Left),
            Key::Right => self.edit_key(EditKey::Right),
            Key::Home => self.edit_key(EditKey::Home),
            Key::End => self.edit_key(EditKey::End),
            Key::Up => self.edit_key(EditKey::Up),
            Key::Down => self.edit_key(EditKey::Down),
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

    use cre::*;

    fn src() -> Element {
        bx!([
            h1!("Title"),
            p!("plain text"),
            link!("https://x.y", "Home"),
            input!(Text, "name", "John"),
            input!(Password, "pw", "hunter2"),
            button!("Go"),
        ])
    }

    fn app(build: impl FnOnce() -> Element, w: usize, h: usize) -> Viewer {
        let doc = Document::from_page(engine::Url::about("blank"), build);
        Viewer::from_document("test.rs", doc, w, h)
    }

    fn key(k: Key) -> KeyPress {
        KeyPress::plain(k)
    }

    fn ch(c: char) -> KeyPress {
        key(Key::Char(c))
    }

    fn ctrl(c: char) -> KeyPress {
        KeyPress::ctrl(Key::Char(c))
    }

    fn row_of(app: &Viewer) -> usize {
        app.cursor.row().unwrap()
    }

    fn text_of(app: &Viewer, row: usize) -> String {
        engine::row_text(&app.page.rows[row]).trim_end().to_string()
    }

    #[test]
    fn starts_in_focused_mode_on_the_first_link() {
        let a = app(src, 40, 20);
        assert_eq!(a.cursor.mode(), Mode::Focused);
        assert_eq!(row_of(&a), 2);
    }

    #[test]
    fn j_and_k_move_between_focusables() {
        let mut a = app(src, 40, 20);
        a.on_key(ch('j'));
        assert_eq!(row_of(&a), 3);
        a.on_key(key(Key::Down));
        a.on_key(key(Key::Tab));
        assert_eq!(row_of(&a), 5);
        assert_eq!(a.on_key(ch('j')), Flow::Continue);
        assert_eq!(row_of(&a), 5);
        a.on_key(key(Key::BackTab));
        a.on_key(ch('k'));
        a.on_key(key(Key::Up));
        assert_eq!(row_of(&a), 2);
    }

    #[test]
    fn m_switches_to_all_mode_and_back() {
        let mut a = app(src, 40, 20);
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
        let mut a = app(src, 40, 20);
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
        let mut a = app(src, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('y'));
        assert_eq!(a.pending_copy.take().as_deref(), Some("John"));
        a.on_key(ch('j'));
        a.on_key(ch('y'));
        assert_eq!(a.pending_copy.take().as_deref(), Some("hunter2"));
    }

    #[test]
    fn copy_on_an_empty_page_reports_nothing_to_copy() {
        let mut a = app(|| p!("x"), 10, 5);
        assert_eq!(a.cursor.row(), None);
        a.on_key(ch('y'));
        assert_eq!(a.pending_copy, None);
        assert_eq!(a.notice.as_deref(), Some("nothing to copy here"));
    }

    #[test]
    fn enter_edits_an_input_and_typing_changes_it() {
        let mut a = app(src, 40, 20);
        a.on_key(ch('j'));
        a.on_key(key(Key::Enter));
        assert_eq!(a.editing(), Some(0));
        a.on_key(ch('!'));
        assert_eq!(text_of(&a, 3), "name: [John!]");
        a.on_key(key(Key::Left));
        a.on_key(key(Key::Backspace));
        assert_eq!(text_of(&a, 3), "name: [Joh!]");
        a.on_key(key(Key::Esc));
        assert_eq!(a.editing(), None);
        assert_eq!(text_of(&a, 3), "name: [Joh!]");
        // The edit survives resizes.
        a.on_resize(60, 20);
        assert_eq!(text_of(&a, 3), "name: [Joh!]");
    }

    #[test]
    fn typed_letters_do_not_trigger_commands_while_editing() {
        let mut a = app(src, 40, 20);
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
        let mut a = app(src, 40, 20);
        a.on_key(key(Key::Enter));
        assert_eq!(a.editing(), None);
    }

    #[test]
    fn i_on_a_link_says_it_is_not_an_input() {
        let mut a = app(src, 40, 20);
        a.on_key(ch('i'));
        assert_eq!(a.editing(), None);
        assert_eq!(
            a.notice.as_deref(),
            Some("not an input (enter edits inputs)")
        );
    }

    #[test]
    fn ctrl_c_copies_while_editing_and_does_nothing_otherwise() {
        let mut a = app(src, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('i'));
        a.on_key(ch('x'));
        assert_eq!(a.on_key(ctrl('c')), Flow::Continue);
        assert_eq!(a.pending_copy.take().as_deref(), Some("Johnx"));
        a.on_key(key(Key::Esc));
        assert_eq!(a.on_key(ctrl('c')), Flow::Continue);
        assert_eq!(a.pending_copy, None);
    }

    #[test]
    fn copy_then_paste_round_trips_through_the_register() {
        let mut a = app(src, 40, 20);
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
        let mut a = app(src, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('i'));
        a.on_key(key(Key::Home));
        a.on_paste("Dr. ".to_string());
        assert_eq!(text_of(&a, 3), "name: [Dr. John]");
        // Line breaks from the clipboard become spaces.
        a.on_paste("a\nb".to_string());
        assert_eq!(text_of(&a, 3), "name: [Dr. a bJohn]");
    }

    #[test]
    fn terminal_paste_on_an_input_enters_it() {
        let mut a = app(src, 40, 20);
        a.on_key(ch('j'));
        a.on_paste("X".to_string());
        assert_eq!(a.editing(), Some(0));
        assert_eq!(text_of(&a, 3), "name: [JohnX]");
    }

    #[test]
    fn paste_is_refused_off_an_input() {
        let mut a = app(src, 40, 20);
        a.on_paste("X".to_string());
        assert_eq!(a.editing(), None);
        assert_eq!(
            a.notice.as_deref(),
            Some("move the cursor onto an input to paste")
        );
        a.on_key(ch('p')); // empty register
        assert_eq!(a.notice.as_deref(), Some("nothing to paste"));
    }

    #[test]
    fn tab_leaves_the_input_and_moves_on() {
        let mut a = app(src, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('i'));
        a.on_key(key(Key::Tab));
        assert_eq!(a.editing(), None);
        assert_eq!(row_of(&a), 4);
    }

    #[test]
    fn quitting_is_the_browsers_business() {
        let mut a = app(src, 40, 20);
        assert_eq!(a.on_key(ch('q')), Flow::Continue);
        assert_eq!(a.on_key(key(Key::Esc)), Flow::Continue);
    }

    #[test]
    fn enter_on_a_link_opens_it_in_the_same_tab() {
        let mut a = app(src, 40, 20);
        assert_eq!(a.current_href().as_deref(), Some("https://x.y"));
        assert_eq!(
            a.on_key(key(Key::Enter)),
            Flow::Open {
                url: "https://x.y".to_string(),
                target: Target::Same
            }
        );
        a.on_key(ch('j'));
        assert_eq!(a.current_href(), None);
    }

    #[test]
    fn enter_on_a_button_runs_its_onclick() {
        let mut a = app(
            || {
                bx!([
                    button!({ onclick: |this: &Element, _: &Root| this.set_text("hit") }, "idle"),
                    button!("plain"),
                ])
            },
            30,
            10,
        );
        assert_eq!(a.on_key(key(Key::Enter)), Flow::Continue);
        assert_eq!(text_of(&a, 0), "[ hit ]");
        assert_eq!(a.cursor.row(), Some(0));
        a.on_key(ch('j'));
        a.on_key(key(Key::Enter));
        assert!(a.notice.as_deref().unwrap().contains("nothing to do"));
    }

    #[test]
    fn a_failing_onclick_is_reported() {
        struct Failing;
        impl engine::Guest for Failing {
            fn tree(&mut self) -> Result<Vec<tree::Element>, String> {
                let mut e = tree::Element::default();
                e.tag = "button".into();
                e.args = vec!["B".into()];
                e.id = 1;
                e.style.insert("onclick".into(), vec!["1".into()]);
                Ok(vec![e])
            }
            fn click(&mut self, _: usize) -> Result<bool, String> {
                Err("wasm trap: unreachable".into())
            }
            fn problems(&mut self) -> Vec<String> {
                vec![]
            }
        }
        let doc = Document::from_guest(engine::Url::about("blank"), Box::new(Failing)).unwrap();
        let mut a = Viewer::from_document("test.rs", doc, 30, 10);
        a.on_key(key(Key::Enter));
        assert!(a.notice.as_deref().unwrap().contains("wasm trap"));
    }

    #[test]
    fn select_row_moves_the_cursor_to_a_stop() {
        let mut a = app(src, 40, 20);
        assert!(a.select_row(5));
        assert_eq!(row_of(&a), 5);
        assert!(!a.select_row(1), "plain text is not a stop in focused mode");
        assert_eq!(row_of(&a), 5);
        assert!(!a.select_row(99));
        a.on_key(ch('j'));
        a.on_key(ch('k'));
        assert!(a.select_row(3));
        a.on_key(key(Key::Enter));
        assert_eq!(a.editing(), Some(0));
        // pressing elsewhere ends the edit
        assert!(a.select_row(2));
        assert_eq!(a.editing(), None);
    }

    #[test]
    fn size_max_fills_the_screen_when_fill_is_on() {
        let mut a = app(|| bx!({ size: "max", bcolor: "blue" }, [p!("x")]), 10, 6);
        assert_eq!(a.page.rows.len(), 5);
        a.set_options(false, false);
        assert_eq!(a.page.rows.len(), 1);
        a.set_options(true, false);
        a.on_resize(10, 9);
        assert_eq!(a.page.rows.len(), 8);
    }

    fn long_page() -> Element {
        long_page_with(None)
    }

    fn long_page_with(tail: Option<&str>) -> Element {
        let e = bx!([button!("first")]);
        for i in 0..40 {
            e.push(p!(format!("line {}", i)));
        }
        e.push(button!("last"));
        if let Some(t) = tail {
            e.push(p!(t));
        }
        e
    }

    #[test]
    fn view_keys_scroll_and_pull_the_cursor_onto_a_visible_stop() {
        let mut a = app(long_page, 30, 11); // 10 visible rows, 42 rows in total
        assert_eq!(row_of(&a), 0);
        a.on_key(ch(' '));
        assert_eq!(a.top, 10);
        // The first button scrolled out and no stop is visible, so the cursor stays.
        assert_eq!(row_of(&a), 0);
        a.on_key(key(Key::End));
        assert_eq!(a.top, 32);
        assert_eq!(row_of(&a), 41); // the last button is visible and gets the cursor
        a.on_key(key(Key::Home));
        assert_eq!(a.top, 0);
        assert_eq!(row_of(&a), 0);
        a.on_key(key(Key::PageDown));
        a.on_key(key(Key::PageUp));
        assert_eq!(a.top, 0);
    }

    #[test]
    fn g_and_capital_g_jump_the_cursor_to_the_first_and_last_stop() {
        let mut a = app(src, 40, 20);
        a.on_key(ch('G'));
        assert_eq!(row_of(&a), 5);
        a.on_key(ch('g'));
        assert_eq!(row_of(&a), 2);
        // With text after the last stop, G goes to the stop and the view follows it.
        let mut b = app(|| long_page_with(Some("tail")), 30, 11);
        b.on_key(ch('G'));
        assert_eq!(row_of(&b), 41);
        assert!(b.top <= 41 && 41 < b.top + 10);
        // End scrolls to the very end of the page instead.
        b.on_key(key(Key::End));
        assert_eq!(b.top, b.page.rows.len() - 10);
    }

    #[test]
    fn g_and_capital_g_scroll_when_there_are_no_stops() {
        let mut a = app(|| bx!([p!("a"), p!("b"), p!("c"), p!("d")]), 10, 3);
        a.on_key(ch('G'));
        assert_eq!(a.top, 2);
        a.on_key(ch('g'));
        assert_eq!(a.top, 0);
    }

    #[test]
    fn j_and_k_without_stops_scroll_the_view() {
        let mut a = app(|| bx!([p!("a"), p!("b"), p!("c"), p!("d")]), 10, 3); // 2 visible rows
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
        let mut a = app(long_page, 30, 11);
        a.on_key(ch('j')); // first -> last button
        assert_eq!(row_of(&a), 41);
        assert_eq!(a.top, 32);
        a.on_key(ch('k'));
        assert_eq!(row_of(&a), 0);
        assert_eq!(a.top, 0);
    }

    #[test]
    fn all_mode_walks_every_line_and_scrolls() {
        let mut a = app(long_page, 30, 6); // 5 visible rows
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
        let mut a = app(
            || input!({ size: "10" }, Text, "name", "hello world foo bar baz"),
            10,
            3,
        ); // 2 visible rows
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
        let mut a = app(src, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('j'));
        assert_eq!(a.cursor.current(&a.page).unwrap().input, Some(1));
        a.on_resize(8, 20);
        assert_eq!(a.cursor.current(&a.page).unwrap().input, Some(1));
        assert!(a.page.rows.len() > 6);
    }

    #[test]
    fn a_page_that_fails_layout_shows_the_error_instead_of_crashing() {
        let a = app(|| p!({ size: "wide" }, "x"), 10, 5);
        assert!(a.notice.as_deref().unwrap().contains("not a valid size"));
        assert!(a.page.rows.is_empty());
    }

    #[test]
    fn copy_selection_copies_the_line_or_the_edited_value() {
        let mut a = app(src, 40, 20);
        a.copy_selection();
        assert_eq!(a.take_copy().as_deref(), Some("Home"));
        assert_eq!(a.take_copy(), None);
        a.on_key(ch('j'));
        a.on_key(ch('i'));
        a.on_key(ch('x'));
        a.copy_selection();
        assert_eq!(a.take_copy().as_deref(), Some("Johnx"));
    }

    #[test]
    fn scroll_lines_moves_the_view_and_clamps() {
        let mut a = app(long_page, 30, 11);
        a.scroll_lines(3);
        assert_eq!(a.top, 3);
        a.scroll_lines(-10);
        assert_eq!(a.top, 0);
        a.scroll_lines(1000);
        assert_eq!(a.top, 32);
    }

    #[test]
    fn status_describes_the_state() {
        let mut a = app(src, 40, 20);
        let s = a.status();
        assert!(s.contains("[focused]") && s.contains("line 3/6"), "{s}");
        a.on_key(ch('m'));
        assert_eq!(a.status(), "cursor mode: all");
        a.on_key(ch('g'));
        a.on_key(ch('j'));
        a.on_key(ch('j'));
        a.on_key(ch('j'));
        a.on_key(ch('i'));
        assert!(a.status().starts_with("editing input 1"));
    }

    #[test]
    fn keys_with_ctrl_or_alt_never_type_text() {
        let mut a = app(src, 40, 20);
        a.on_key(ch('j'));
        a.on_key(ch('i'));
        a.on_key(KeyPress::new(
            Key::Char('z'),
            Mods {
                ctrl: false,
                alt: true,
                shift: false,
            },
        ));
        a.on_key(ctrl('z'));
        assert_eq!(text_of(&a, 3), "name: [John]");
        a.on_key(ctrl('a'));
        a.on_key(ch('>'));
        assert_eq!(text_of(&a, 3), "name: [>John]");
        a.on_key(ctrl('e'));
        a.on_key(ch('<'));
        assert_eq!(text_of(&a, 3), "name: [>John<]");
    }
}
