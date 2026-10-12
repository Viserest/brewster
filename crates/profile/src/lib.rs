//! Profiles: a sandbox holding everything a person's browser remembers: settings, the
//! open tabs, the applications with plugins, and (later) the viewing history.
//!
//! The profile file (TOML, JSON or YAML) stores all of it. Plugin code is kept as one file
//! per plugin in a directory of its own, inside the profile's sandbox: under the user's home
//! directory by default, or in `/tmp/brewster/plugins/` (gone after a reboot) with
//! `storage = "tmp"`. Two profiles never share plugins.

use std::path::{Path, PathBuf};

use plugin::{Registry, Uuid};
use settings::{Format, Map, Settings, Value};

pub const VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedTab {
    pub url: String,
    pub title: String,
}

/// Viewing history. Nothing records it yet; the field is read and written so profiles made
/// by a later version keep theirs.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    /// Seconds since the Unix epoch.
    pub visited: i64,
}

/// One application and the plugins installed for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppRecord {
    /// `authority` or `authority/path`.
    pub app: String,
    pub plugins: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    Home,
    Tmp,
}

impl Storage {
    pub fn name(self) -> &'static str {
        match self {
            Storage::Home => "home",
            Storage::Tmp => "tmp",
        }
    }
}

/// Where profiles live. Tests pass their own; apps use [`Paths::from_env`].
#[derive(Debug, Clone)]
pub struct Paths {
    pub home: Option<PathBuf>,
    pub tmp: PathBuf,
    pub config: Option<PathBuf>,
}

impl Paths {
    pub fn from_env() -> Paths {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let home = var("HOME").or_else(|| var("USERPROFILE"));
        let config = var("XDG_CONFIG_HOME")
            .map(|c| c.join("brewster"))
            .or_else(|| home.as_ref().map(|h| h.join(".config").join("brewster")));
        Paths {
            home,
            tmp: PathBuf::from("/tmp/brewster/plugins"),
            config,
        }
    }

    /// The profile file `name.toml` in the config directory.
    pub fn profile_file(&self, name: &str) -> Result<PathBuf, String> {
        check_name(name)?;
        let dir = self
            .config
            .as_ref()
            .ok_or("no home directory: pass a profile file with --profile")?;
        Ok(dir.join("profiles").join(format!("{}.toml", name)))
    }

    /// The directory a profile keeps its plugins in.
    pub fn plugin_dir(&self, name: &str, storage: Storage) -> Result<PathBuf, String> {
        check_name(name)?;
        let base = match (storage, &self.home) {
            (Storage::Home, Some(h)) => h.join(".brewster").join("plugins"),
            // No home directory: fall back to the temporary place rather than fail.
            _ => self.tmp.clone(),
        };
        Ok(base.join(name))
    }
}

/// Profile names become directory names: letters, digits, `_` and `-` only.
pub fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!(
            "`{}` is not a valid profile name (use letters, digits, `_` and `-`, at most 64)",
            name
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    pub storage: Storage,
    pub settings: Settings,
    pub tabs: Vec<SavedTab>,
    pub active_tab: usize,
    pub history: Vec<HistoryEntry>,
    pub applications: Vec<AppRecord>,
    /// The file this profile is saved to; `None` for a throwaway profile.
    pub path: Option<PathBuf>,
    /// Keys in the file this version does not know; written back unchanged.
    extra: Map,
}

impl Profile {
    pub fn new(name: &str) -> Result<Profile, String> {
        check_name(name)?;
        Ok(Profile {
            name: name.to_string(),
            storage: Storage::Home,
            settings: Settings::default(),
            tabs: Vec::new(),
            active_tab: 0,
            history: Vec::new(),
            applications: Vec::new(),
            path: None,
            extra: Map::new(),
        })
    }

    /// Reads a profile file. Problems in single values are returned as warnings and the
    /// rest of the file still loads; an unreadable or malformed file is an error.
    pub fn load(path: &Path) -> Result<(Profile, Vec<String>), String> {
        let v = settings::read_file(path)?;
        let (mut p, warnings) = Profile::from_value(&v, path)?;
        p.path = Some(path.to_path_buf());
        Ok((p, warnings))
    }

    /// Loads the profile file if it exists, otherwise starts a new profile that will be
    /// saved there.
    pub fn load_or_new(path: &Path, default_name: &str) -> Result<(Profile, Vec<String>), String> {
        if path.exists() {
            return Profile::load(path);
        }
        let mut p = Profile::new(default_name)?;
        p.path = Some(path.to_path_buf());
        Ok((p, Vec::new()))
    }

    pub fn from_value(v: &Value, origin: &Path) -> Result<(Profile, Vec<String>), String> {
        let mut warnings = Vec::new();
        let mut extra = v.as_map().cloned().unwrap_or_default();
        let mut take = |k: &str| extra.remove(k);

        if let Some(ver) = take("version") {
            match ver.as_int() {
                Some(n) if n <= VERSION => {}
                _ => {
                    return Err(format!(
                        "{}: written by a newer brewster (profile version {})",
                        origin.display(),
                        ver.display()
                    ));
                }
            }
        }
        let default_name = origin
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| check_name(s).is_ok())
            .unwrap_or("default")
            .to_string();
        let name = match take("name") {
            Some(Value::Str(s)) => {
                check_name(&s)?;
                s
            }
            Some(other) => {
                return Err(format!(
                    "{}: `name` must be text, not {}",
                    origin.display(),
                    other.display()
                ));
            }
            None => default_name,
        };
        let storage = match take("storage") {
            None => Storage::Home,
            Some(Value::Str(s)) if s == "home" => Storage::Home,
            Some(Value::Str(s)) if s == "tmp" => Storage::Tmp,
            Some(other) => {
                warnings.push(format!(
                    "storage must be \"home\" or \"tmp\", not {}; using \"home\"",
                    other.display()
                ));
                Storage::Home
            }
        };
        let settings = match take("settings") {
            Some(s @ Value::Map(_)) => {
                let (s, w) = Settings::from_value(&s);
                warnings.extend(w);
                s
            }
            Some(_) => {
                warnings.push("`settings` must be a table; using the defaults".to_string());
                Settings::default()
            }
            None => Settings::default(),
        };

        let mut tabs = Vec::new();
        match take("tabs") {
            Some(Value::List(l)) => {
                for (i, t) in l.iter().enumerate() {
                    match t.get("url").and_then(Value::as_str) {
                        Some(url) => tabs.push(SavedTab {
                            url: url.to_string(),
                            title: t
                                .get("title")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                        }),
                        None => warnings.push(format!("tab {} has no url and was skipped", i + 1)),
                    }
                }
            }
            Some(_) => warnings.push("`tabs` must be a list; no tabs restored".to_string()),
            None => {}
        }
        let active_tab = match take("active_tab") {
            Some(Value::Int(i)) if i >= 0 && (i as usize) < tabs.len() => i as usize,
            _ => 0,
        };

        let mut history = Vec::new();
        if let Some(Value::List(l)) = take("history") {
            for h in &l {
                if let Some(url) = h.get("url").and_then(Value::as_str) {
                    history.push(HistoryEntry {
                        url: url.to_string(),
                        title: h
                            .get("title")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        visited: h.get("visited").and_then(Value::as_int).unwrap_or(0),
                    });
                }
            }
        }

        let mut applications = Vec::new();
        if let Some(Value::List(l)) = take("applications") {
            for a in &l {
                if let Some(app) = a.get("app").and_then(Value::as_str) {
                    let plugins = a
                        .get("plugins")
                        .and_then(Value::as_list)
                        .map(|l| {
                            l.iter()
                                .filter_map(|x| x.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    applications.push(AppRecord {
                        app: app.to_string(),
                        plugins,
                    });
                }
            }
        }

        Ok((
            Profile {
                name,
                storage,
                settings,
                tabs,
                active_tab,
                history,
                applications,
                path: None,
                extra,
            },
            warnings,
        ))
    }

    /// Everything, as a value ready to be written in any format. Only settings that differ
    /// from the defaults are written, so new defaults reach existing profiles.
    pub fn to_value(&self) -> Value {
        let mut m = self.extra.clone();
        m.insert("version".into(), Value::Int(VERSION));
        m.insert("name".into(), Value::str(&self.name));
        m.insert("storage".into(), Value::str(self.storage.name()));
        m.insert("settings".into(), self.settings.changed());
        let map = |pairs: Vec<(&str, Value)>| {
            Value::Map(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
        };
        m.insert(
            "tabs".into(),
            Value::List(
                self.tabs
                    .iter()
                    .map(|t| {
                        map(vec![
                            ("url", Value::str(&t.url)),
                            ("title", Value::str(&t.title)),
                        ])
                    })
                    .collect(),
            ),
        );
        m.insert("active_tab".into(), Value::Int(self.active_tab as i64));
        m.insert(
            "history".into(),
            Value::List(
                self.history
                    .iter()
                    .map(|h| {
                        map(vec![
                            ("url", Value::str(&h.url)),
                            ("title", Value::str(&h.title)),
                            ("visited", Value::Int(h.visited)),
                        ])
                    })
                    .collect(),
            ),
        );
        m.insert(
            "applications".into(),
            Value::List(
                self.applications
                    .iter()
                    .map(|a| {
                        map(vec![
                            ("app", Value::str(&a.app)),
                            (
                                "plugins",
                                Value::List(a.plugins.iter().map(Value::str).collect()),
                            ),
                        ])
                    })
                    .collect(),
            ),
        );
        Value::Map(m)
    }

    /// Saves to the profile's own file.
    pub fn save(&self) -> Result<(), String> {
        let path = self
            .path
            .as_ref()
            .ok_or("this profile has no file to save to")?;
        settings::write_file(path, &self.to_value())
    }

    /// Saves in another file (the format follows its extension) and makes it the profile's file.
    pub fn save_as(&mut self, path: &Path) -> Result<(), String> {
        if Format::from_path(path).is_none() {
            return Err(format!(
                "{}: use a .toml, .json, .yaml or .yml file",
                path.display()
            ));
        }
        settings::write_file(path, &self.to_value())?;
        self.path = Some(path.to_path_buf());
        Ok(())
    }

    /// Opens this profile's plugin store.
    pub fn open_registry(&self, paths: &Paths) -> Result<Registry, String> {
        let dir = paths.plugin_dir(&self.name, self.storage)?;
        Registry::open(&dir).map_err(|e| e.to_string())
    }

    /// Refreshes the application list from the plugin store.
    pub fn sync_applications(&mut self, reg: &Registry) {
        self.applications = reg
            .applications()
            .into_iter()
            .map(|key| AppRecord {
                app: key.to_string(),
                plugins: reg_ids(reg, &key),
            })
            .collect();
    }
}

fn reg_ids(reg: &Registry, key: &plugin::AppKey) -> Vec<String> {
    let mut ids: Vec<(u64, Uuid)> = Vec::new();
    for e in reg.entries() {
        if &e.key == key {
            ids.push((e.seq, e.id));
        }
    }
    ids.sort();
    ids.into_iter().map(|(_, id)| id.to_string()).collect()
}

#[cfg(test)]
mod tests;
