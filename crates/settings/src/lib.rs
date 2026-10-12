//! Settings: nested key/value pairs read from and written to TOML, JSON or YAML, a schema
//! of the options brewster understands, and a search over them.

mod json;
mod schema;
mod toml;
mod value;
mod yaml;

pub use schema::{DefaultValue, Kind, OPTIONS, OptionDef, option, search};
pub use value::{Map, Value, escape};

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Toml,
    Json,
    Yaml,
}

impl Format {
    pub fn from_path(p: &Path) -> Option<Format> {
        match p.extension()?.to_str()?.to_lowercase().as_str() {
            "toml" => Some(Format::Toml),
            "json" => Some(Format::Json),
            "yaml" | "yml" => Some(Format::Yaml),
            _ => None,
        }
    }

    pub fn from_name(n: &str) -> Option<Format> {
        match n.to_lowercase().as_str() {
            "toml" => Some(Format::Toml),
            "json" => Some(Format::Json),
            "yaml" | "yml" => Some(Format::Yaml),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Toml => "toml",
            Format::Json => "json",
            Format::Yaml => "yaml",
        }
    }

    pub fn parse(self, src: &str) -> Result<Value, String> {
        let v = match self {
            Format::Toml => toml::parse(src)?,
            Format::Json => json::parse(src)?,
            Format::Yaml => yaml::parse(src)?,
        };
        match v {
            Value::Map(_) => Ok(v),
            Value::Null => Ok(Value::map()),
            _ => Err("the file must hold key/value pairs at the top level".to_string()),
        }
    }

    pub fn write(self, v: &Value) -> String {
        match self {
            Format::Toml => toml::write(v),
            Format::Json => json::write(v),
            Format::Yaml => yaml::write(v),
        }
    }
}

/// Reads a settings-style file; the format comes from the extension.
pub fn read_file(path: &Path) -> Result<Value, String> {
    let fmt = Format::from_path(path)
        .ok_or_else(|| format!("{}: use a .toml, .json, .yaml or .yml file", path.display()))?;
    let src = fs::read_to_string(path).map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    fmt.parse(&src).map_err(|e| format!("{}: {}", path.display(), e))
}

/// Writes a file next to its final name first, so a crash cannot leave half a file.
pub fn write_file(path: &Path, v: &Value) -> Result<(), String> {
    let fmt = Format::from_path(path)
        .ok_or_else(|| format!("{}: use a .toml, .json, .yaml or .yml file", path.display()))?;
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {}", dir.display(), e))?;
        }
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    fs::write(&tmp, fmt.write(v)).map_err(|e| format!("cannot write {}: {}", tmp.display(), e))?;
    fs::rename(&tmp, path).map_err(|e| format!("cannot write {}: {}", path.display(), e))
}

/// The settings in effect: every known option (default or overridden) plus any extra keys
/// the user wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    values: Value,
}

impl std::default::Default for Settings {
    fn default() -> Settings {
        let mut values = Value::map();
        for o in OPTIONS {
            values.set(o.key, o.default_value()).expect("schema keys are valid");
        }
        Settings { values }
    }
}

impl Settings {
    /// Defaults with `over` (a parsed file's `settings` table) on top. Values that fail
    /// their option's check are skipped and reported, so one typo does not lose the rest.
    pub fn from_value(over: &Value) -> (Settings, Vec<String>) {
        let mut s = Settings::default();
        let mut problems = Vec::new();
        for (key, v) in over.flatten() {
            match option(&key) {
                Some(o) => match o.check(v) {
                    Ok(()) => {
                        let _ = s.values.set(&key, v.clone());
                    }
                    Err(e) => problems.push(e),
                },
                None => {
                    // Unknown keys are kept (a newer version may know them); empty tables
                    // carry nothing worth keeping.
                    if !matches!(v, Value::Map(_)) && s.values.set(&key, v.clone()).is_err() {
                        problems.push(format!("cannot keep `{}`", key));
                    }
                }
            }
        }
        (s, problems)
    }

    pub fn as_value(&self) -> &Value {
        &self.values
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    pub fn str(&self, key: &str) -> String {
        match self.get(key) {
            Some(Value::Str(s)) => s.clone(),
            _ => option(key).map(|o| o.default_value().display()).unwrap_or_default(),
        }
    }

    pub fn bool(&self, key: &str) -> bool {
        match self.get(key) {
            Some(Value::Bool(b)) => *b,
            _ => option(key).and_then(|o| o.default_value().as_bool()).unwrap_or(false),
        }
    }

    pub fn int(&self, key: &str) -> i64 {
        match self.get(key) {
            Some(Value::Int(i)) => *i,
            _ => option(key).and_then(|o| o.default_value().as_int()).unwrap_or(0),
        }
    }

    pub fn list(&self, key: &str) -> Vec<String> {
        let from = |v: &Value| -> Vec<String> {
            v.as_list().map(|l| l.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
        };
        match self.get(key) {
            Some(v @ Value::List(_)) => from(v),
            _ => option(key).map(|o| from(&o.default_value())).unwrap_or_default(),
        }
    }

    /// Changes one option. Known options are checked; unknown keys are stored as given.
    pub fn set(&mut self, key: &str, v: Value) -> Result<(), String> {
        if let Some(o) = option(key) {
            o.check(&v)?;
        }
        self.values.set(key, v)
    }

    /// Back to the default for a known option; unknown keys are removed.
    pub fn reset(&mut self, key: &str) {
        match option(key) {
            Some(o) => {
                let _ = self.values.set(key, o.default_value());
            }
            None => {
                self.values.remove(key);
            }
        }
    }

    /// Only what differs from the defaults, for saving.
    pub fn changed(&self) -> Value {
        let mut out = Value::map();
        for (key, v) in self.values.flatten() {
            let is_default = option(&key).is_some_and(|o| o.default_value() == *v);
            let empty_table = matches!(v, Value::Map(m) if m.is_empty());
            if !is_default && !empty_table {
                let _ = out.set(&key, v.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
