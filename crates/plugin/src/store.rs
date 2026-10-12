//! The set of installed plugins, optionally saved as one file per plugin.

use crate::{AppKey, Plugin, PluginError, Uuid, program_from_bytes};

use std::{
    fs,
    path::{Path, PathBuf},
};

use location::{Loader, Url};
use parser::Stmt;

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: Uuid,
    pub key: AppKey,
    pub plugin: Plugin,
    /// Insertion order across the whole registry.
    pub seq: u64,
    /// The last successfully fetched bytes of a URL plugin, kept for offline use.
    pub cache: Option<Vec<u8>>,
}

#[derive(Debug, Default)]
pub struct Registry {
    dir: Option<PathBuf>,
    entries: Vec<Entry>,
    next_seq: u64,
    /// Files that could not be read when opening a directory.
    pub warnings: Vec<String>,
}

fn storage<E: std::fmt::Display>(what: &str, e: E) -> PluginError {
    PluginError::Storage(format!("{}: {}", what, e))
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

impl Registry {
    /// A registry that keeps nothing on disk.
    pub fn in_memory() -> Registry {
        Registry::default()
    }

    /// Opens (creating if needed) a directory of saved plugins.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Registry, PluginError> {
        let dir = dir.into();
        fs::create_dir_all(&dir).map_err(|e| storage(&dir.display().to_string(), e))?;
        let mut reg = Registry {
            dir: Some(dir.clone()),
            ..Registry::default()
        };
        let rd = fs::read_dir(&dir).map_err(|e| storage(&dir.display().to_string(), e))?;
        for item in rd.flatten() {
            let path = item.path();
            if path.extension().and_then(|e| e.to_str()) != Some("plugin") {
                continue;
            }
            match fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| decode_entry(&t))
            {
                Ok(e) => reg.entries.push(e),
                Err(e) => reg.warnings.push(format!("{}: {}", path.display(), e)),
            }
        }
        reg.entries.sort_by_key(|e| e.seq);
        reg.next_seq = reg.entries.last().map_or(0, |e| e.seq + 1);
        Ok(reg)
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    fn file_of(&self, id: Uuid) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(format!("{}.plugin", id)))
    }

    fn save(&self, e: &Entry) -> Result<(), PluginError> {
        let Some(path) = self.file_of(e.id) else {
            return Ok(());
        };
        let tmp = path.with_extension("plugin.tmp");
        fs::write(&tmp, encode_entry(e)).map_err(|er| storage(&tmp.display().to_string(), er))?;
        fs::rename(&tmp, &path).map_err(|er| storage(&path.display().to_string(), er))
    }

    fn erase(&self, id: Uuid) -> Result<(), PluginError> {
        if let Some(path) = self.file_of(id) {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(storage(&path.display().to_string(), e)),
            }
        }
        Ok(())
    }

    /// Validates a plugin and resolves a URL against `base`. Returns the plugin to store
    /// and, for URLs, the bytes fetched now (if the network is up).
    fn prepare(
        plugin: Plugin,
        base: &Url,
        loader: &dyn Loader,
    ) -> Result<(Plugin, Option<Vec<u8>>), PluginError> {
        plugin.validate()?;
        match plugin {
            Plugin::Url(u) => {
                let abs = base
                    .resolve(&u)
                    .map_err(|e| PluginError::Fetch(format!("{}: {}", u, e)))?;
                let cache = match loader.load_bytes(&abs) {
                    Ok(bytes) => {
                        program_from_bytes(&bytes)?;
                        Some(bytes)
                    }
                    // Offline now: keep the plugin and fetch it on the first page load.
                    Err(_) => None,
                };
                Ok((Plugin::Url(abs.to_string()), cache))
            }
            other => Ok((other, None)),
        }
    }

    /// Installs a plugin for an application and returns its id. It does not run now.
    pub fn install(
        &mut self,
        key: AppKey,
        plugin: Plugin,
        base: &Url,
        loader: &dyn Loader,
    ) -> Result<Uuid, PluginError> {
        let (plugin, cache) = Registry::prepare(plugin, base, loader)?;
        let e = Entry {
            id: Uuid::new_v4(),
            key,
            plugin,
            seq: self.next_seq,
            cache,
        };
        self.save(&e)?;
        self.next_seq += 1;
        let id = e.id;
        self.entries.push(e);
        Ok(id)
    }

    /// The plugins of an application, in the order they run.
    pub fn list(&self, key: &AppKey) -> Vec<Plugin> {
        self.entries
            .iter()
            .filter(|e| &e.key == key)
            .map(|e| e.plugin.clone())
            .collect()
    }

    pub fn get(&self, id: Uuid) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Replaces a plugin's code, keeping its id, application and place in the order.
    pub fn update(
        &mut self,
        id: Uuid,
        plugin: Plugin,
        base: &Url,
        loader: &dyn Loader,
    ) -> Result<(), PluginError> {
        let idx = self
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or(PluginError::NotFound(id))?;
        let (plugin, cache) = Registry::prepare(plugin, base, loader)?;
        let mut e = self.entries[idx].clone();
        e.plugin = plugin;
        e.cache = cache;
        self.save(&e)?;
        self.entries[idx] = e;
        Ok(())
    }

    pub fn delete(&mut self, id: Uuid) -> Result<(), PluginError> {
        let idx = self
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or(PluginError::NotFound(id))?;
        self.erase(id)?;
        self.entries.remove(idx);
        Ok(())
    }

    /// Removes every plugin of an application.
    pub fn clear(&mut self, key: &AppKey) -> Result<(), PluginError> {
        let ids: Vec<Uuid> = self
            .entries
            .iter()
            .filter(|e| &e.key == key)
            .map(|e| e.id)
            .collect();
        for id in ids {
            self.delete(id)?;
        }
        Ok(())
    }

    /// Every application that has plugins, sorted.
    pub fn applications(&self) -> Vec<AppKey> {
        let mut keys: Vec<AppKey> = self.entries.iter().map(|e| e.key.clone()).collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// Every entry in insertion order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Plugins whose application covers `url`, in insertion order.
    pub fn entries_for(&self, url: &Url) -> Vec<&Entry> {
        self.entries.iter().filter(|e| e.key.matches(url)).collect()
    }

    /// The programs to run for a page, in insertion order. A URL plugin is fetched again;
    /// if that fails the saved copy is used, so pages keep working offline.
    pub fn programs_for(
        &mut self,
        url: &Url,
        loader: &dyn Loader,
    ) -> Vec<(Uuid, Result<Vec<Stmt>, PluginError>)> {
        let ids: Vec<Uuid> = self.entries_for(url).iter().map(|e| e.id).collect();
        let mut out = Vec::new();
        for id in ids {
            let idx = self
                .entries
                .iter()
                .position(|e| e.id == id)
                .expect("id came from entries");
            let result = match self.entries[idx].plugin.clone() {
                Plugin::Url(u) => {
                    let fresh = Plugin::fetch(&u, url, loader);
                    match fresh {
                        Ok(bytes) => match program_from_bytes(&bytes) {
                            Ok(p) => {
                                if self.entries[idx].cache.as_deref() != Some(&bytes[..]) {
                                    self.entries[idx].cache = Some(bytes);
                                    // Failing to save the cache must not stop the page.
                                    let _ = self.save(&self.entries[idx]);
                                }
                                Ok(p)
                            }
                            Err(e) => Err(e),
                        },
                        Err(fetch_err) => match &self.entries[idx].cache {
                            Some(bytes) => program_from_bytes(bytes),
                            None => Err(fetch_err),
                        },
                    }
                }
                other => other
                    .local_program()
                    .expect("non-URL plugins carry their code"),
            };
            out.push((id, result));
        }
        out
    }
}

// ── file format ─────────────────────────────────────────────────────

const HEADER: &str = "brewster-plugin 1";

fn encode_entry(e: &Entry) -> String {
    let mut s = format!(
        "{}\nid: {}\nseq: {}\nauthority: {}\npath: {}\n",
        HEADER,
        e.id,
        e.seq,
        e.key.authority(),
        e.key.path()
    );
    let body = match &e.plugin {
        Plugin::Compiled(b) => {
            s.push_str("kind: compiled\n");
            hex(b)
        }
        Plugin::Source(t) => {
            s.push_str("kind: source\n");
            t.clone()
        }
        Plugin::Url(u) => {
            s.push_str(&format!("kind: url\nurl: {}\n", u));
            e.cache.as_deref().map(hex).unwrap_or_default()
        }
    };
    s.push('\n');
    s.push_str(&body);
    s
}

fn decode_entry(text: &str) -> Result<Entry, String> {
    let (head, body) = text.split_once("\n\n").unwrap_or((text, ""));
    let mut lines = head.lines();
    if lines.next() != Some(HEADER) {
        return Err("not a brewster plugin file".to_string());
    }
    let mut id = None;
    let mut seq = None;
    let mut authority = None;
    let mut path = None;
    let mut kind = None;
    let mut url = None;
    for l in lines {
        let Some((k, v)) = l.split_once(": ").or_else(|| l.split_once(':')) else {
            continue;
        };
        match k {
            "id" => id = Uuid::parse(v),
            "seq" => seq = v.trim().parse::<u64>().ok(),
            "authority" => authority = Some(v.to_string()),
            "path" => path = Some(v.to_string()),
            "kind" => kind = Some(v.trim().to_string()),
            "url" => url = Some(v.trim().to_string()),
            _ => {}
        }
    }
    let id = id.ok_or("missing or bad `id`")?;
    let seq = seq.ok_or("missing or bad `seq`")?;
    let authority = authority.unwrap_or_default();
    let path = path.ok_or("missing `path`")?;
    if !path.starts_with('/') {
        return Err("bad `path`".to_string());
    }
    let (plugin, cache) = match kind.as_deref() {
        Some("compiled") => (
            Plugin::Compiled(unhex(body).ok_or("bad compiled body")?),
            None,
        ),
        Some("source") => (Plugin::Source(body.to_string()), None),
        Some("url") => {
            let cache = if body.trim().is_empty() {
                None
            } else {
                Some(unhex(body).ok_or("bad cache body")?)
            };
            (Plugin::Url(url.ok_or("missing `url`")?), cache)
        }
        _ => return Err("missing or unknown `kind`".to_string()),
    };
    Ok(Entry {
        id,
        key: AppKey::from_parts(&authority, &path),
        plugin,
        seq,
        cache,
    })
}
