//! Browser: tabs, links, key bindings, the settings box and the profile, on top of the
//! `viewer` (one per tab). A frontend (terminal or window) feeds it keys, mouse events,
//! pastes and resizes, paints [`Browser::screen`] and carries out [`Effect`]s.

mod keymap;
pub mod launch;
mod overlay;
mod pages;
mod screen;

pub use keymap::{ACTIONS, Action, Binding, Button, Keymap, label, parse_binding};
pub use screen::{Cell, Look, Screen};
pub use viewer::{Key, KeyPress, Mods};

use std::cell::RefCell;
use std::rc::Rc;

use engine::{Document, Host, Loader, Mode, Registry, Url};
use overlay::Overlay;
use profile::{Profile, SavedTab};
use settings::Settings;
use viewer::{Flow, Target, Viewer, paint::row_pieces};

/// Something the frontend has to do on the browser's behalf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Put this text on the system clipboard.
    Copy(String),
    /// Open this address in a new window (only produced when [`Capabilities::windows`] is on).
    NewWindow(String),
    Quit,
}

/// What a frontend can show. Both default to off, which suits a terminal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// A second window can be opened (otherwise "new window" opens a tab).
    pub windows: bool,
    /// Text can be drawn at different sizes (headers get their font sizes).
    pub scaling: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    Down(Button),
    /// Wheel: positive scrolls down.
    Wheel(isize),
}

/// A mouse event in screen cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    pub col: usize,
    pub row: usize,
    pub kind: MouseKind,
    pub mods: Mods,
}

struct Tab {
    viewer: Viewer,
    url: Url,
    title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

impl Rect {
    fn contains(&self, col: usize, row: usize) -> bool {
        col >= self.x && col < self.x + self.w && row >= self.y && row < self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BarPos {
    Top,
    Bottom,
    Left,
    Right,
}

struct Geometry {
    bar: Option<(Rect, BarPos)>,
    content: Rect,
    status_row: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemKind {
    Title(usize),
    Close(usize),
    New,
}

struct TabItem {
    x: usize,
    y: usize,
    w: usize,
    kind: ItemKind,
    text: String,
    active: bool,
}

pub struct Browser {
    pub profile: Profile,
    /// The settings as last loaded from or saved to the profile file.
    saved_settings: Settings,
    registry: Rc<RefCell<Registry>>,
    loader: Rc<dyn Loader>,
    host: Rc<Host>,
    tabs: Vec<Tab>,
    active: usize,
    cols: usize,
    rows: usize,
    keymap: Keymap,
    overlay: Option<Overlay>,
    notice: Option<String>,
    effects: Vec<Effect>,
    caps: Capabilities,
    quit: bool,
    follower: bool,
}

fn clip(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else if max == 0 {
        String::new()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

impl Browser {
    /// A browser with no tabs yet: call [`Browser::start`]. Pages are compiled with the
    /// `rustc` found in the environment; run them with [`Browser::with_host`] and a wasm
    /// engine (this build has none).
    pub fn new(
        profile: Profile,
        registry: Registry,
        loader: Rc<dyn Loader>,
        cols: usize,
        rows: usize,
    ) -> Browser {
        Browser::with_host(profile, registry, loader, Rc::new(Host::detect()), cols, rows)
    }

    /// Like [`Browser::new`] with the compiler and wasm engine to use for pages.
    pub fn with_host(
        profile: Profile,
        registry: Registry,
        loader: Rc<dyn Loader>,
        host: Rc<Host>,
        cols: usize,
        rows: usize,
    ) -> Browser {
        let (keymap, problems) = Keymap::from_settings(&profile.settings);
        let saved_settings = profile.settings.clone();
        Browser {
            profile,
            saved_settings,
            registry: Rc::new(RefCell::new(registry)),
            loader,
            host,
            tabs: Vec::new(),
            active: 0,
            cols: cols.max(1),
            rows: rows.max(2),
            keymap,
            overlay: None,
            notice: problems.first().cloned(),
            effects: Vec::new(),
            caps: Capabilities::default(),
            quit: false,
            follower: false,
        }
    }

    /// A follower window (opened from another one) does not remember its tabs at quit, so
    /// it cannot overwrite the first window's session. Saving with the save key still works.
    pub fn set_follower(&mut self, follower: bool) {
        self.follower = follower;
    }

    /// What the frontend can do (see [`Capabilities`]).
    pub fn set_capabilities(&mut self, caps: Capabilities) {
        self.caps = caps;
        self.apply_settings();
    }

    /// Opens the first tabs: the given addresses, else the last session's tabs, else the
    /// home page.
    pub fn start(&mut self, urls: &[String]) {
        if !urls.is_empty() {
            for u in urls {
                self.push_tab(Url::from_input(u));
            }
            self.active = 0;
        } else if self.settings().bool("general.restore_tabs") && !self.profile.tabs.is_empty() {
            let saved = self.profile.tabs.clone();
            for t in &saved {
                self.push_tab(Url::from_input(&t.url));
            }
            self.active = self
                .profile
                .active_tab
                .min(self.tabs.len().saturating_sub(1));
        } else {
            let home = self.settings().str("general.homepage");
            self.push_tab(Url::from_input(&home));
        }
        self.relayout_all();
    }

    pub fn settings(&self) -> &Settings {
        &self.profile.settings
    }

    pub(crate) fn settings_mut(&mut self) -> &mut Settings {
        &mut self.profile.settings
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn active_tab(&self) -> usize {
        self.active
    }

    pub fn tab_title(&self, i: usize) -> Option<&str> {
        self.tabs.get(i).map(|t| t.title.as_str())
    }

    pub fn tab_url(&self, i: usize) -> Option<String> {
        self.tabs.get(i).map(|t| t.url.to_string())
    }

    /// The active tab's viewer (for tests and frontends that need its state).
    pub fn viewer(&self) -> &Viewer {
        &self.tabs[self.active].viewer
    }

    pub fn settings_open(&self) -> bool {
        matches!(self.overlay, Some(Overlay::Settings(_)))
    }

    pub fn overlay_open(&self) -> bool {
        self.overlay.is_some()
    }

    // ── loading ─────────────────────────────────────────────────────

    fn hints(&self) -> Vec<String> {
        let k = |a: Action, what: &str| {
            self.keymap
                .first_label(a)
                .map(|l| format!("{} {}", l, what))
        };
        [
            k(Action::OpenUrl, "open an address or file"),
            k(Action::Settings, "settings"),
            k(Action::NewTab, "new tab"),
            k(Action::CloseTab, "close tab"),
            k(Action::Quit, "quit"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// The page at `url`: one of the browser's own `about:` pages, or a module fetched
    /// (and compiled when it is source) and run.
    fn document_for(&self, url: &Url) -> Result<Document, String> {
        if url.scheme() == "about" {
            return match url.path() {
                "blank" => Ok(Document::blank()),
                "newtab" => {
                    let apps: Vec<String> = self
                        .registry
                        .borrow()
                        .applications()
                        .iter()
                        .map(|a| a.to_string())
                        .collect();
                    let hints = self.hints();
                    Ok(Document::from_page(url.clone(), move || {
                        pages::new_tab(&apps, &hints)
                    }))
                }
                other => Err(format!("there is no page about:{}", other)),
            };
        }
        Document::load(
            url.clone(),
            self.registry.clone(),
            self.loader.clone(),
            self.host.clone(),
        )
    }

    /// Loads `url` into a ready tab. A page that cannot be loaded becomes an error page, so
    /// the tab always has something to show.
    fn make_tab(&mut self, url: &Url) -> Tab {
        let g = self.geometry();
        let (doc, notice) = match self.document_for(url) {
            Ok(d) => (d, None),
            Err(e) => {
                let (u, m) = (url.to_string(), e.clone());
                let d = Document::from_page(url.clone(), move || pages::error(&u, &m));
                (d, Some(e))
            }
        };
        let mut notice = notice;
        if notice.is_none() {
            if let Some(first) = doc.diagnostics().first() {
                let n = doc.diagnostics().len();
                notice = Some(format!(
                    "{} plugin problem{}: {}",
                    n,
                    if n == 1 { "" } else { "s" },
                    first
                ));
            }
        }
        let title = doc.title().unwrap_or_else(|| url.short_name());
        let mut viewer = Viewer::from_document(&url.to_string(), doc, g.content.w, g.content.h + 1);
        viewer.enter_follows_links = false;
        self.configure(&mut viewer);
        if let Some(n) = notice {
            self.notice = Some(n);
        }
        Tab {
            viewer,
            url: url.clone(),
            title,
        }
    }

    fn configure(&self, v: &mut Viewer) {
        let s = self.settings();
        v.set_options(
            s.bool("view.fill_window"),
            s.bool("view.scale_headers") && self.caps.scaling,
        );
        v.set_mode(if s.str("view.cursor_mode") == "all" {
            Mode::All
        } else {
            Mode::Focused
        });
    }

    fn push_tab(&mut self, url: Url) {
        let tab = self.make_tab(&url);
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    fn navigate(&mut self, url: Url, target: Target) {
        if url.scheme() == "about" && url.path() == "settings" {
            self.open_settings();
            return;
        }
        match target {
            Target::Same => {
                let tab = self.make_tab(&url);
                self.tabs[self.active] = tab;
            }
            Target::Tab => self.insert_tab_after_active(url),
            Target::Window => {
                if self.caps.windows {
                    self.effect(Effect::NewWindow(url.to_string()));
                } else {
                    self.insert_tab_after_active(url);
                    self.notice =
                        Some("this viewer has one window: opened in a new tab".to_string());
                }
            }
        }
        self.relayout_all();
    }

    fn insert_tab_after_active(&mut self, url: Url) {
        let tab = self.make_tab(&url);
        let at = (self.active + 1).min(self.tabs.len());
        self.tabs.insert(at, tab);
        self.active = at;
    }

    pub(crate) fn navigate_input(&mut self, text: &str) {
        self.navigate(Url::from_input(text), Target::Same);
    }

    fn open_link(&mut self, href: &str, target: Target) {
        let base = self.tabs[self.active].url.clone();
        match base.resolve(href) {
            Ok(u) => self.navigate(u, target),
            Err(e) => self.notice = Some(format!("cannot open {}: {}", href, e)),
        }
    }

    fn reload(&mut self) {
        let url = self.tabs[self.active].url.clone();
        let tab = self.make_tab(&url);
        self.tabs[self.active] = tab;
        self.relayout_all();
        if self.notice.is_none() {
            self.notice = Some("reloaded".to_string());
        }
    }

    fn close_tab(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            self.quit_now();
            return;
        }
        self.tabs.remove(i);
        if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1).min(self.tabs.len() - 1);
        }
        self.relayout_all();
    }

    fn new_tab(&mut self) {
        let home = self.settings().str("general.homepage");
        self.insert_tab_after_active(Url::from_input(&home));
        self.relayout_all();
    }

    fn switch(&mut self, delta: isize) {
        let n = self.tabs.len() as isize;
        self.active = (self.active as isize + delta).rem_euclid(n) as usize;
    }

    fn quit_now(&mut self) {
        if let Err(e) = self.autosave() {
            self.notice = Some(e);
        }
        self.quit = true;
        self.effect(Effect::Quit);
    }

    // ── profile ─────────────────────────────────────────────────────

    fn snapshot_tabs(&self, p: &mut Profile) {
        p.tabs = self
            .tabs
            .iter()
            .filter(|t| !(t.url.scheme() == "about" && t.url.path() == "settings"))
            .map(|t| SavedTab {
                url: t.url.to_string(),
                title: t.title.clone(),
            })
            .collect();
        p.active_tab = self.active.min(p.tabs.len().saturating_sub(1));
        p.sync_applications(&self.registry.borrow());
    }

    /// Writes the profile file with the current settings, tabs and applications.
    pub fn save_profile(&mut self) -> Result<String, String> {
        let mut p = self.profile.clone();
        self.snapshot_tabs(&mut p);
        p.save()?;
        self.profile = p;
        self.saved_settings = self.profile.settings.clone();
        let path = self
            .profile
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        Ok(format!("saved to {}", path))
    }

    /// At quit: remembers tabs and applications, but keeps the settings that are on disk, so
    /// settings changed only for this session stay session-only.
    fn autosave(&mut self) -> Result<(), String> {
        if self.profile.path.is_none() || self.follower {
            return Ok(());
        }
        let mut p = self.profile.clone();
        p.settings = self.saved_settings.clone();
        self.snapshot_tabs(&mut p);
        p.save()
    }

    /// Whether settings differ from what the profile file holds.
    pub fn settings_unsaved(&self) -> bool {
        self.profile.settings != self.saved_settings
    }

    pub(crate) fn apply_settings(&mut self) {
        let (km, problems) = Keymap::from_settings(&self.profile.settings);
        self.keymap = km;
        if let Some(p) = problems.first() {
            self.notice = Some(p.clone());
        }
        let (fill, scale, mode) = {
            let s = self.settings();
            (
                s.bool("view.fill_window"),
                s.bool("view.scale_headers") && self.caps.scaling,
                if s.str("view.cursor_mode") == "all" {
                    Mode::All
                } else {
                    Mode::Focused
                },
            )
        };
        for t in &mut self.tabs {
            t.viewer.set_mode(mode);
            t.viewer.set_options(fill, scale);
        }
        self.relayout_all();
    }

    // ── geometry ────────────────────────────────────────────────────

    fn geometry(&self) -> Geometry {
        let s = self.settings();
        let show = match s.str("tabs.show").as_str() {
            "never" => false,
            "multiple" => self.tabs.len() > 1,
            _ => true,
        };
        let status_row = self.rows - 1;
        let body_h = self.rows - 1;
        let full = Rect {
            x: 0,
            y: 0,
            w: self.cols,
            h: body_h,
        };
        if !show || body_h < 2 {
            return Geometry {
                bar: None,
                content: full,
                status_row,
            };
        }
        let pos = match s.str("tabs.position").as_str() {
            "bottom" => BarPos::Bottom,
            "left" => BarPos::Left,
            "right" => BarPos::Right,
            _ => BarPos::Top,
        };
        match pos {
            BarPos::Top => Geometry {
                bar: Some((
                    Rect {
                        x: 0,
                        y: 0,
                        w: self.cols,
                        h: 1,
                    },
                    pos,
                )),
                content: Rect {
                    x: 0,
                    y: 1,
                    w: self.cols,
                    h: body_h - 1,
                },
                status_row,
            },
            BarPos::Bottom => Geometry {
                bar: Some((
                    Rect {
                        x: 0,
                        y: body_h - 1,
                        w: self.cols,
                        h: 1,
                    },
                    pos,
                )),
                content: Rect {
                    x: 0,
                    y: 0,
                    w: self.cols,
                    h: body_h - 1,
                },
                status_row,
            },
            BarPos::Left | BarPos::Right => {
                let want = s.int("tabs.sidebar_width").max(1) as usize;
                // Leave at least half the width for the page.
                let sw = want.min(self.cols / 2).max(1);
                if self.cols < 4 {
                    return Geometry {
                        bar: None,
                        content: full,
                        status_row,
                    };
                }
                if pos == BarPos::Left {
                    Geometry {
                        bar: Some((
                            Rect {
                                x: 0,
                                y: 0,
                                w: sw,
                                h: body_h,
                            },
                            pos,
                        )),
                        content: Rect {
                            x: sw,
                            y: 0,
                            w: self.cols - sw,
                            h: body_h,
                        },
                        status_row,
                    }
                } else {
                    Geometry {
                        bar: Some((
                            Rect {
                                x: self.cols - sw,
                                y: 0,
                                w: sw,
                                h: body_h,
                            },
                            pos,
                        )),
                        content: Rect {
                            x: 0,
                            y: 0,
                            w: self.cols - sw,
                            h: body_h,
                        },
                        status_row,
                    }
                }
            }
        }
    }

    pub fn on_resize(&mut self, cols: usize, rows: usize) {
        self.cols = cols.max(1);
        self.rows = rows.max(2);
        self.relayout_all();
    }

    fn relayout_all(&mut self) {
        let g = self.geometry();
        for t in &mut self.tabs {
            t.viewer
                .on_resize(g.content.w.max(1), g.content.h.max(1) + 1);
        }
    }

    /// The clickable items of the tab bar.
    fn tab_items(&self, bar: Rect, pos: BarPos) -> Vec<TabItem> {
        let mut items = Vec::new();
        let n = self.tabs.len();
        match pos {
            BarPos::Top | BarPos::Bottom => {
                let tw = self.settings().int("tabs.title_width").max(6) as usize;
                // " title ×" with the title clipped so the whole item is `tw` cells at most.
                let labels: Vec<String> = self
                    .tabs
                    .iter()
                    .map(|t| format!(" {} ", clip(&t.title, tw.saturating_sub(4).max(1))))
                    .collect();
                let widths: Vec<usize> = labels.iter().map(|l| l.chars().count() + 1).collect();
                let new_w = 3;
                // Scroll so that the active tab is visible.
                let mut start = 0;
                while start < self.active
                    && widths[start..=self.active].iter().sum::<usize>() + new_w > bar.w
                {
                    start += 1;
                }
                let mut x = bar.x;
                for i in start..n {
                    if x + widths[i] > bar.x + bar.w {
                        break;
                    }
                    let label_w = widths[i] - 1;
                    items.push(TabItem {
                        x,
                        y: bar.y,
                        w: label_w,
                        kind: ItemKind::Title(i),
                        text: labels[i].clone(),
                        active: i == self.active,
                    });
                    items.push(TabItem {
                        x: x + label_w,
                        y: bar.y,
                        w: 1,
                        kind: ItemKind::Close(i),
                        text: "×".to_string(),
                        active: i == self.active,
                    });
                    x += widths[i];
                }
                if x + new_w <= bar.x + bar.w {
                    items.push(TabItem {
                        x,
                        y: bar.y,
                        w: new_w,
                        kind: ItemKind::New,
                        text: " + ".to_string(),
                        active: false,
                    });
                }
            }
            BarPos::Left | BarPos::Right => {
                // The column next to the page is the separator.
                let inner = bar.w.saturating_sub(1).max(1);
                let rows = bar.h;
                let total = n + 1; // tabs and the "new tab" row
                let mut start = 0;
                if self.active >= rows {
                    start = self.active + 1 - rows;
                }
                let x0 = if pos == BarPos::Left {
                    bar.x
                } else {
                    bar.x + 1
                };
                for (row, i) in (start..total).enumerate() {
                    if row >= rows {
                        break;
                    }
                    let y = bar.y + row;
                    if i == n {
                        items.push(TabItem {
                            x: x0,
                            y,
                            w: inner,
                            kind: ItemKind::New,
                            text: " + new tab".to_string(),
                            active: false,
                        });
                    } else {
                        let title_w = inner.saturating_sub(2).max(1);
                        let text = format!(
                            " {}",
                            clip(&self.tabs[i].title, title_w.saturating_sub(1).max(1))
                        );
                        items.push(TabItem {
                            x: x0,
                            y,
                            w: inner.saturating_sub(1).max(1),
                            kind: ItemKind::Title(i),
                            text,
                            active: i == self.active,
                        });
                        items.push(TabItem {
                            x: x0 + inner.saturating_sub(1).max(1),
                            y,
                            w: 1,
                            kind: ItemKind::Close(i),
                            text: "×".to_string(),
                            active: i == self.active,
                        });
                    }
                }
            }
        }
        items
    }

    // ── input ───────────────────────────────────────────────────────

    pub fn on_paste(&mut self, text: String) {
        self.notice = None;
        if self.overlay.is_some() {
            self.overlay_paste(&text);
            return;
        }
        self.tabs[self.active].viewer.on_paste(text);
        self.collect_copy();
    }

    fn collect_copy(&mut self) {
        if let Some(t) = self.tabs[self.active].viewer.take_copy() {
            self.effect(Effect::Copy(t));
        }
    }

    /// The frontend's copy command (Ctrl+C in a window): copies from the page.
    pub fn copy_selection(&mut self) {
        self.tabs[self.active].viewer.copy_selection();
        self.collect_copy();
    }

    pub fn on_key(&mut self, key: KeyPress) {
        self.notice = None;
        if self.overlay.is_some() {
            self.overlay_key(key);
            return;
        }
        let editing = self.tabs[self.active].viewer.editing().is_some();
        let bound = if editing && !Keymap::works_while_typing(key) {
            None
        } else {
            self.keymap.key(key)
        };
        // Ctrl+C always quits outside text editing, so a bad key map cannot lock you in.
        let hard_quit = !editing && key.mods.ctrl && key.key == Key::Char('c');
        if hard_quit {
            self.quit_now();
            return;
        }
        if let Some(action) = bound {
            if self.run_action(action) {
                return;
            }
        }
        let flow = self.tabs[self.active].viewer.on_key(key);
        self.collect_copy();
        self.handle_flow(flow);
    }

    fn handle_flow(&mut self, flow: Flow) {
        match flow {
            Flow::Continue => {}
            Flow::Open { url, target } => self.open_link(&url, target),
        }
    }

    /// Runs an action; false means the key was not used and goes on to the page.
    fn run_action(&mut self, action: Action) -> bool {
        match action {
            Action::Quit => self.quit_now(),
            Action::Settings => self.open_settings(),
            Action::NewTab => self.new_tab(),
            Action::CloseTab => self.close_tab(self.active),
            Action::NextTab => self.switch(1),
            Action::PrevTab => self.switch(-1),
            Action::Reload => self.reload(),
            Action::OpenUrl => self.open_prompt(),
            Action::SaveSettings => {
                self.notice = Some(match self.save_profile() {
                    Ok(m) => m,
                    Err(e) => e,
                });
            }
            Action::LinkOpen | Action::LinkNewTab | Action::LinkNewWindow => {
                let Some(href) = self.tabs[self.active].viewer.current_href() else {
                    return false;
                };
                let target = match action {
                    Action::LinkOpen => Target::Same,
                    Action::LinkNewTab => Target::Tab,
                    _ => Target::Window,
                };
                self.open_link(&href, target);
            }
        }
        true
    }

    pub fn on_mouse(&mut self, ev: MouseEvent) {
        self.notice = None;
        if self.overlay.is_some() {
            match ev.kind {
                MouseKind::Down(Button::Left) => self.overlay_click(ev.col, ev.row),
                MouseKind::Wheel(d) => self.overlay_wheel(d),
                _ => {}
            }
            return;
        }
        let g = self.geometry();
        if let Some((bar, pos)) = g.bar {
            if bar.contains(ev.col, ev.row) {
                if let MouseKind::Down(b) = ev.kind {
                    self.click_tab_bar(bar, pos, ev.col, ev.row, b);
                }
                return;
            }
        }
        if !g.content.contains(ev.col, ev.row) {
            return;
        }
        match ev.kind {
            MouseKind::Wheel(d) => self.tabs[self.active].viewer.scroll_lines(d * 3),
            MouseKind::Down(button) => {
                let row = self.tabs[self.active].viewer.top + (ev.row - g.content.y);
                let selected = self.tabs[self.active].viewer.select_row(row);
                if !selected {
                    return;
                }
                let href = self.tabs[self.active].viewer.current_href();
                match (href, self.keymap.click(button, ev.mods)) {
                    (Some(href), Some(action)) => {
                        let target = match action {
                            Action::LinkOpen => Target::Same,
                            Action::LinkNewTab => Target::Tab,
                            _ => Target::Window,
                        };
                        self.open_link(&href, target);
                    }
                    // A link pressed with a chord that is not bound: just select it.
                    (Some(_), None) => {}
                    (None, _) => {
                        if button == Button::Left {
                            let flow = self.tabs[self.active].viewer.activate();
                            self.handle_flow(flow);
                        }
                    }
                }
            }
        }
    }

    fn click_tab_bar(&mut self, bar: Rect, pos: BarPos, col: usize, row: usize, button: Button) {
        let items = self.tab_items(bar, pos);
        let hit = items
            .iter()
            .find(|it| it.y == row && col >= it.x && col < it.x + it.w);
        let Some(hit) = hit else { return };
        match (hit.kind, button) {
            (ItemKind::Title(i), Button::Left) => self.active = i,
            (ItemKind::Title(i), Button::Middle)
            | (ItemKind::Close(i), Button::Left | Button::Middle) => self.close_tab(i),
            (ItemKind::New, Button::Left) => self.new_tab(),
            _ => {}
        }
    }

    // ── painting ────────────────────────────────────────────────────

    /// The status line: a message if there is one, else where you are and how to ask for help.
    pub fn status(&self) -> String {
        if let Some(n) = &self.notice {
            return n.replace('\n', " ");
        }
        let t = &self.tabs[self.active];
        if let Some(n) = &t.viewer.notice {
            return n.replace('\n', " ");
        }
        if let Some(id) = t.viewer.editing() {
            return format!(
                "editing input {}  enter/esc done  ctrl-c copy  ctrl-v paste",
                id + 1
            );
        }
        let pos = match t.viewer.cursor.row() {
            Some(r) => format!("line {}/{}", r + 1, t.viewer.page.rows.len()),
            None => "no cursor".to_string(),
        };
        let mut s = format!("{}  [{}] {}", t.url, t.viewer.cursor.mode().name(), pos);
        if self.settings_unsaved() {
            s.push_str("  (settings not saved)");
        }
        if let Some(k) = self.keymap.first_label(Action::Settings) {
            s.push_str(&format!("  {} settings", k));
        }
        s
    }

    /// Composes everything to draw: tab bar, page, status line and any box on top.
    pub fn screen(&self) -> Screen {
        let mut s = Screen::new(self.cols, self.rows);
        let g = self.geometry();
        if let Some((bar, pos)) = g.bar {
            self.paint_tab_bar(&mut s, bar, pos);
        }
        self.paint_page(&mut s, g.content);
        let status = clip(&self.status(), self.cols);
        s.fill(0, g.status_row, self.cols, 1, Look::REVERSE);
        s.text(0, g.status_row, &status, Look::REVERSE, self.cols);
        self.paint_overlay(&mut s);
        s
    }

    fn paint_tab_bar(&self, s: &mut Screen, bar: Rect, pos: BarPos) {
        s.fill(bar.x, bar.y, bar.w, bar.h, Look::PLAIN);
        for it in self.tab_items(bar, pos) {
            let look = if it.active {
                Look::BOLD_REVERSE
            } else {
                Look::PLAIN
            };
            s.text(it.x, it.y, &it.text, look, it.w);
        }
        // The separator between a sidebar and the page.
        match pos {
            BarPos::Left => {
                for y in bar.y..bar.y + bar.h {
                    s.set(
                        bar.x + bar.w - 1,
                        y,
                        Cell {
                            ch: '│',
                            ..Cell::default()
                        },
                    );
                }
            }
            BarPos::Right => {
                for y in bar.y..bar.y + bar.h {
                    s.set(
                        bar.x,
                        y,
                        Cell {
                            ch: '│',
                            ..Cell::default()
                        },
                    );
                }
            }
            _ => {}
        }
    }

    fn paint_page(&self, s: &mut Screen, area: Rect) {
        let v = &self.tabs[self.active].viewer;
        let hl = v.highlight();
        let editing = v.editing().is_some();
        for y in 0..area.h {
            let idx = v.top + y;
            let Some(row) = v.page.rows.get(idx) else {
                break;
            };
            let mut x = 0usize;
            for (span, reversed) in row_pieces(row, idx, hl.as_ref(), editing) {
                let w = span.scale.max(1) as usize;
                for (k, ch) in span.text.chars().enumerate() {
                    let cx = x + k * w;
                    if cx >= area.w {
                        break;
                    }
                    s.set(
                        area.x + cx,
                        area.y + y,
                        Cell {
                            ch,
                            fg: span.fg,
                            bg: span.bg,
                            bold: span.bold,
                            underline: span.underline,
                            reverse: reversed,
                            scale: span.scale.max(1),
                            cover: false,
                        },
                    );
                    for dx in 1..w {
                        if cx + dx >= area.w {
                            break;
                        }
                        s.set(
                            area.x + cx + dx,
                            area.y + y,
                            Cell {
                                ch: ' ',
                                fg: span.fg,
                                bg: span.bg,
                                reverse: reversed,
                                cover: true,
                                ..Cell::default()
                            },
                        );
                    }
                }
                x += span.text.chars().count() * w;
            }
        }
    }
}

#[cfg(test)]
mod tests;
