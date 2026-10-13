//! A page and its plugins as one document.
//!
//! Every guest numbers its own elements from 1, so [`Session`] gives them room: the id the
//! rest of the browser sees is `guest << 24 | local`. The page is guest 0, so its ids are
//! unchanged. Plugin elements come after the page's, in the order the plugins were added.

use tree::Element;

use crate::Guest;

const SHIFT: usize = 24;
const MASK: usize = (1 << SHIFT) - 1;

struct Entry {
    guest: Box<dyn Guest>,
    /// `None` for the page, the plugin's name for a plugin.
    label: Option<String>,
}

#[derive(Default)]
pub struct Session {
    guests: Vec<Entry>,
    tree: Vec<Element>,
    /// Problems the guests reported and plugin modules that failed, oldest first.
    pub diagnostics: Vec<String>,
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

    /// Adds the page. It must be added first.
    pub fn set_page(&mut self, guest: Box<dyn Guest>) {
        assert!(self.guests.is_empty(), "the page comes first");
        self.guests.push(Entry { guest, label: None });
    }

    pub fn add_plugin(&mut self, label: &str, guest: Box<dyn Guest>) {
        self.guests.push(Entry {
            guest,
            label: Some(label.to_string()),
        });
    }

    /// Asks every guest for its tree again. A page that fails is an error; a plugin that
    /// fails is left out and noted in `diagnostics`.
    pub fn refresh(&mut self) -> Result<(), String> {
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
