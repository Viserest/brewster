//! Plugins and applications.
//!
//! A *plugin* is code a page (or the user) hands to the browser: a `cre` page module. It
//! comes in three forms:
//!
//! 1. [`Plugin::Wasm`]: already compiled, so nothing needs to be built here;
//! 2. [`Plugin::Source`]: Rust source, compiled to wasm on this computer (and cached);
//! 3. [`Plugin::Url`]: a URL of either of the above, fetched when needed. When the source
//!    cannot be compiled here, the pre-compiled `.wasm` beside it is requested instead.
//!
//! An *application* ([`AppKey`]) is scoped like a service worker: an authority (domain or
//! address) plus a path. Plugins installed for an application run, in the order they were
//! installed, on every page whose URL falls under it. They are saved to disk, so they run
//! again after a reload even when the connection is gone.

mod compile;
mod store;
pub use compile::{Compiler, load_module, module_from_bytes, wasm_sibling};
pub use store::{Entry, Registry, hex, unhex};

use std::{
    collections::hash_map::RandomState,
    fmt,
    hash::{BuildHasher, Hasher},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use location::{Loader, Url};

// ── Uuid ────────────────────────────────────────────────────────────

/// A random (version 4) universally unique identifier.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uuid(u128);

fn random_u64() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let mut h = RandomState::new().build_hasher();
    h.write_u64(nanos);
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    h.write_u32(std::process::id());
    h.finish()
}

impl Uuid {
    pub fn new_v4() -> Uuid {
        let hi = random_u64() as u128;
        let lo = random_u64() as u128;
        let mut v = (hi << 64) | lo;
        v &= !(0xF_u128 << 76);
        v |= 0x4_u128 << 76; // version 4
        v &= !(0x3_u128 << 62);
        v |= 0x2_u128 << 62; // RFC 4122 variant
        Uuid(v)
    }

    /// Parses `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`.
    pub fn parse(s: &str) -> Option<Uuid> {
        let s = s.trim();
        if s.len() != 36 {
            return None;
        }
        let mut hex = String::with_capacity(32);
        for (i, c) in s.chars().enumerate() {
            if matches!(i, 8 | 13 | 18 | 23) {
                if c != '-' {
                    return None;
                }
            } else if c.is_ascii_hexdigit() {
                hex.push(c);
            } else {
                return None;
            }
        }
        u128::from_str_radix(&hex, 16).ok().map(Uuid)
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let h = format!("{:032x}", self.0);
        write!(
            f,
            "{}-{}-{}-{}-{}",
            &h[..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..]
        )
    }
}

impl fmt::Debug for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Uuid({})", self)
    }
}

// ── errors ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    /// The code does not compile and no pre-compiled module could be had.
    Compile(String),
    /// A plugin URL could not be fetched and nothing is cached.
    Fetch(String),
    /// Saving or loading plugin files failed.
    Storage(String),
    /// No plugin has this id.
    NotFound(Uuid),
    /// The application path is not usable.
    InvalidApp(String),
    /// The plugin started but its code failed.
    Runtime(String),
}

impl PluginError {
    /// A short name for scripts to branch on.
    pub fn kind(&self) -> &'static str {
        match self {
            PluginError::Compile(_) => "Compile",
            PluginError::Fetch(_) => "Fetch",
            PluginError::Storage(_) => "Storage",
            PluginError::NotFound(_) => "NotFound",
            PluginError::InvalidApp(_) => "InvalidApp",
            PluginError::Runtime(_) => "Runtime",
        }
    }
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginError::Compile(m) => write!(f, "cannot compile plugin: {}", m),
            PluginError::Fetch(m) => write!(f, "cannot fetch plugin: {}", m),
            PluginError::Storage(m) => write!(f, "plugin storage: {}", m),
            PluginError::NotFound(id) => write!(f, "no plugin with id {}", id),
            PluginError::InvalidApp(m) => write!(f, "invalid application: {}", m),
            PluginError::Runtime(m) => write!(f, "plugin failed: {}", m),
        }
    }
}

impl std::error::Error for PluginError {}

// ── Plugin ──────────────────────────────────────────────────────────

/// The first bytes of every wasm module.
pub const WASM_MAGIC: &[u8] = b"\0asm";

pub fn is_wasm(bytes: &[u8]) -> bool {
    bytes.starts_with(WASM_MAGIC)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plugin {
    /// An already compiled module (`.wasm`), usually made by the plugin's author.
    Wasm(Vec<u8>),
    /// Rust source (a `cre` page module), compiled to wasm on this computer.
    Source(String),
    /// Where to get either of the above. Relative URLs are resolved against the page that
    /// uses the plugin. If the source behind it cannot be compiled here, the `.wasm` next
    /// to it is fetched instead.
    Url(String),
}

impl Plugin {
    /// Checks what can be checked without compiling: that wasm is wasm and that source or
    /// a URL is not empty. Source is compiled when the plugin first runs.
    pub fn validate(&self) -> Result<(), PluginError> {
        match self {
            Plugin::Wasm(b) if !is_wasm(b) => Err(PluginError::Compile(
                "not a wasm module (bad magic)".into(),
            )),
            Plugin::Source(s) if s.trim().is_empty() => {
                Err(PluginError::Compile("empty plugin source".into()))
            }
            Plugin::Url(u) if u.trim().is_empty() => {
                Err(PluginError::Fetch("empty plugin URL".into()))
            }
            _ => Ok(()),
        }
    }

    /// Fetches a URL plugin's bytes, resolved against `base`.
    pub fn fetch(url: &str, base: &Url, loader: &dyn Loader) -> Result<Vec<u8>, PluginError> {
        let target = base
            .resolve(url)
            .map_err(|e| PluginError::Fetch(format!("{}: {}", url, e)))?;
        loader
            .load_bytes(&target)
            .map_err(|e| PluginError::Fetch(format!("{}: {}", target, e)))
    }

    /// The wasm module of this plugin: compiled, fetched or both. No caching of fetched
    /// bytes here (the registry does that); compiled output is cached by the compiler.
    pub fn module(
        &self,
        base: &Url,
        loader: &dyn Loader,
        compiler: &Compiler,
    ) -> Result<Vec<u8>, PluginError> {
        match self {
            Plugin::Wasm(b) => Ok(b.clone()),
            Plugin::Source(s) => compiler.compile(s).map_err(PluginError::Compile),
            Plugin::Url(u) => {
                let target = base
                    .resolve(u)
                    .map_err(|e| PluginError::Fetch(format!("{}: {}", u, e)))?;
                load_module(&target, loader, compiler)
            }
        }
    }
}

// ── AppKey ──────────────────────────────────────────────────────────

/// An application: an authority plus a path, `domain.com/<url_app>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AppKey {
    authority: String,
    path: String,
}

impl AppKey {
    /// The application `url_app` under `authority`. `url_app` is a path such as `shop`,
    /// `/shop/` or `shop/cart`; empty or `/` is the whole domain. A leading `authority/`
    /// naming the same authority is accepted (`domain.com/shop`).
    pub fn new(authority: &str, url_app: &str) -> Result<AppKey, PluginError> {
        let authority = authority.to_ascii_lowercase();
        let mut p = url_app.trim();
        if let Some(rest) = p.strip_prefix(&format!("{}/", authority)) {
            p = rest;
        } else if p == authority && !authority.is_empty() {
            p = "";
        }
        if p.contains('?') || p.contains('#') || p.contains('\0') || p.contains('\\') {
            return Err(PluginError::InvalidApp(format!(
                "`{}` is not a plain path",
                url_app
            )));
        }
        let mut parts: Vec<&str> = Vec::new();
        for seg in p.split('/') {
            match seg {
                "" | "." => {}
                ".." => {
                    return Err(PluginError::InvalidApp(format!(
                        "`{}` contains `..`",
                        url_app
                    )));
                }
                s => parts.push(s),
            }
        }
        Ok(AppKey {
            authority,
            path: format!("/{}", parts.join("/")),
        })
    }

    pub fn from_parts(authority: &str, path: &str) -> AppKey {
        AppKey {
            authority: authority.to_string(),
            path: path.to_string(),
        }
    }

    pub fn authority(&self) -> &str {
        &self.authority
    }

    /// Always starts with `/`; `/` alone is the whole authority.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Whether a page belongs to this application: the same authority and a path that is
    /// this one or below it (on a segment boundary, so `/shop` does not cover `/shopping`).
    pub fn matches(&self, url: &Url) -> bool {
        if url.authority() != self.authority {
            return false;
        }
        if self.path == "/" {
            return true;
        }
        let p = url.path();
        p == self.path
            || p.strip_prefix(&self.path)
                .is_some_and(|rest| rest.starts_with('/'))
    }
}

impl fmt::Display for AppKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path == "/" {
            write!(f, "{}", self.authority)
        } else {
            write!(f, "{}{}", self.authority, self.path)
        }
    }
}

#[cfg(test)]
mod tests;
