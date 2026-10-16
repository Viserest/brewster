//! A page and its plugins as one document.
//!
//! Every guest numbers its own elements from 1, so [`Session`] gives them room: the id the
//! rest of the browser sees is `guest << 24 | local`. The page is guest 0, so its ids are
//! unchanged. Plugin elements come after the page's, in the order the plugins were added.

use std::rc::Rc;

use tree::Element;

use crate::{Guest, NetClient};

const SHIFT: usize = 24;
const MASK: usize = (1 << SHIFT) - 1;

struct Entry {
    guest: Box<dyn Guest>,
    /// `None` for the page, the plugin's name for a plugin.
    label: Option<String>,
}

/// How many times a page may ask for the network after getting answers before the
/// session stops asking: a page that requests forever must not freeze the browser.
const MAX_ROUNDS: usize = 16;

#[derive(Default)]
pub struct Session {
    guests: Vec<Entry>,
    tree: Vec<Element>,
    url: Option<cre::Url>,
    net: Option<Rc<dyn NetClient>>,
    /// Problems the guests reported and plugin modules that failed, oldest first.
    pub diagnostics: Vec<String>,
}

fn note(label: &Option<String>, msg: String) -> String {
    match label {
        None => msg,
        Some(l) => format!("plugin {}: {}", l, msg),
    }
}

fn remap(e: &mut Element, guest: usize) {
    e.id = (guest << SHIFT) | (e.id & MASK);
    for c in &mut e.children {
        remap(c, guest);
    }
}

fn find(es: &[Element], id: usize) -> Option<&Element> {
    es.iter().find_map(|e| {
        if e.id == id {
            Some(e)
        } else {
            find(&e.children, id)
        }
    })
}

impl Session {
    pub fn new() -> Session {
        Session::default()
    }

    /// Who carries out the requests pages ask for. Without one every request fails.
    pub fn set_net(&mut self, net: Rc<dyn NetClient>) {
        self.net = Some(net);
    }

    /// The address of the tab: told to every guest now, and to every guest added later,
    /// before it first runs. Call [`refresh`](Session::refresh) to see what the pages do
    /// with it.
    pub fn set_url(&mut self, url: cre::Url) {
        for entry in &mut self.guests {
            if let Err(e) = entry.guest.set_url(&url) {
                self.diagnostics.push(note(&entry.label, e));
            }
        }
        self.url = Some(url);
    }

    /// Adds the page. It must be added first.
    pub fn set_page(&mut self, guest: Box<dyn Guest>) {
        assert!(self.guests.is_empty(), "the page comes first");
        self.push(guest, None);
    }

    pub fn add_plugin(&mut self, label: &str, guest: Box<dyn Guest>) {
        self.push(guest, Some(label.to_string()));
    }

    fn push(&mut self, mut guest: Box<dyn Guest>, label: Option<String>) {
        if let Some(url) = &self.url {
            if let Err(e) = guest.set_url(url) {
                self.diagnostics.push(note(&label, e));
            }
        }
        self.guests.push(Entry { guest, label });
    }

    /// The user edited the input `id`: the guest that owns it is told, so a handler that
    /// reads the form sees what was typed.
    pub fn set_input(&mut self, id: usize, value: &str) {
        if let Some(entry) = self.guests.get_mut(id >> SHIFT) {
            if let Err(e) = entry.guest.set_input(id & MASK, value) {
                self.diagnostics.push(note(&entry.label, e));
            }
        }
    }

    /// Asks every guest for its tree again, after carrying out the network requests they
    /// have made and giving them the answers (which may make more requests, and change the
    /// tree). A page that fails is an error; a plugin that fails is left out and noted in
    /// `diagnostics`.
    pub fn refresh(&mut self) -> Result<(), String> {
        for _ in 0..MAX_ROUNDS {
            self.collect()?;
            if !self.pump() {
                return Ok(());
            }
        }
        self.diagnostics
            .push("a page kept making network requests: stopped asking".to_string());
        self.collect()
    }

    /// Carries out every queued request. `true` when there was any.
    fn pump(&mut self) -> bool {
        let net = self.net.clone();
        let mut any = false;
        for entry in &mut self.guests {
            let reqs = match entry.guest.net_take() {
                Ok(r) => r,
                Err(e) => {
                    self.diagnostics.push(note(&entry.label, e));
                    continue;
                }
            };
            for req in reqs {
                any = true;
                let result = match &net {
                    Some(n) => n.fetch(&req),
                    None => Err(cre::NetworkError::Failed(
                        "this page has no network access".to_string(),
                    )),
                };
                if let Err(e) = entry.guest.net_done(req.id, result) {
                    self.diagnostics.push(note(&entry.label, e));
                }
            }
        }
        any
    }

    fn collect(&mut self) -> Result<(), String> {
        let mut all = Vec::new();
        let mut notes = Vec::new();
        for (i, entry) in self.guests.iter_mut().enumerate() {
            match entry.guest.tree() {
                Ok(mut t) => {
                    for e in &mut t {
                        remap(e, i);
                    }
                    all.extend(t);
                }
                Err(e) => match &entry.label {
                    None => return Err(e),
                    Some(l) => notes.push(format!("plugin {}: {}", l, e)),
                },
            }
            for p in entry.guest.problems() {
                notes.push(match &entry.label {
                    None => p,
                    Some(l) => format!("plugin {}: {}", l, p),
                });
            }
        }
        self.tree = all;
        self.diagnostics.extend(notes);
        Ok(())
    }

    /// The elements layout draws, as of the last [`refresh`](Session::refresh).
    pub fn tree(&self) -> &[Element] {
        &self.tree
    }

    pub fn is_clickable(&self, id: usize) -> bool {
        find(&self.tree, id).is_some_and(|e| e.style.contains_key("onclick"))
    }

    /// Runs the handler of the element `id`, then refreshes. `Ok(false)` when it has none.
    pub fn click(&mut self, id: usize) -> Result<bool, String> {
        if !self.is_clickable(id) {
            return Ok(false);
        }
        let Some(entry) = self.guests.get_mut(id >> SHIFT) else {
            return Ok(false);
        };
        let handled = entry.guest.click(id & MASK);
        // Whatever the handler changed before failing is still shown.
        let refreshed = self.refresh();
        let handled = handled?;
        refreshed?;
        Ok(handled)
    }
}
