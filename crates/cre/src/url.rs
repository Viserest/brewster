//! The page's address, as `root().url()` gives it, and the HTTP methods a route or a
//! request can have.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Method {
    #[default]
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

const fn eq_ci(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i].to_ascii_lowercase() != b[i].to_ascii_lowercase() {
            return false;
        }
        i += 1;
    }
    true
}

impl Method {
    /// `get`, `GET` and `Get` are all `Method::Get`.
    pub const fn parse(s: &str) -> Option<Method> {
        if eq_ci(s, "get") {
            Some(Method::Get)
        } else if eq_ci(s, "head") {
            Some(Method::Head)
        } else if eq_ci(s, "post") {
            Some(Method::Post)
        } else if eq_ci(s, "put") {
            Some(Method::Put)
        } else if eq_ci(s, "patch") {
            Some(Method::Patch)
        } else if eq_ci(s, "delete") {
            Some(Method::Delete)
        } else if eq_ci(s, "options") {
            Some(Method::Options)
        } else {
            None
        }
    }

    /// For the macros: in a `const` this is a compile error for an unknown method.
    pub const fn parse_const(s: &str) -> Method {
        match Method::parse(s) {
            Some(m) => m,
            None => panic!("unknown HTTP method: use get, post, put, patch, delete, head or options"),
        }
    }

    /// `"GET"`, `"POST"` ...
    pub const fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Head => "HEAD",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Options => "OPTIONS",
        }
    }

    /// The byte the page protocol sends for a method.
    pub const fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Option<Method> {
        Some(match c {
            0 => Method::Get,
            1 => Method::Head,
            2 => Method::Post,
            3 => Method::Put,
            4 => Method::Patch,
            5 => Method::Delete,
            6 => Method::Options,
            _ => return None,
        })
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where the page is: the address of the browser's tab, kept up to date by the browser.
/// `method` is how the page was reached (always `Get` today: the browser only follows
/// links).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Url {
    pub method: Method,
    /// The whole address as the browser shows it.
    pub href: String,
    /// `http`, `file`, `about` ...
    pub scheme: String,
    /// `host[:port]`; empty for `file:` and `about:`.
    pub authority: String,
    /// Always starts with `/` for hierarchical addresses.
    pub path: String,
    /// Without the `?`.
    pub query: Option<String>,
    /// Without the `#`.
    pub fragment: Option<String>,
}

impl Url {
    pub fn parse(href: &str) -> Url {
        Url::parse_with(Method::Get, href)
    }

    pub fn parse_with(method: Method, href: &str) -> Url {
        let (rest, fragment) = match href.split_once('#') {
            Some((r, f)) => (r, Some(f.to_string())),
            None => (href, None),
        };
        let (rest, query) = match rest.split_once('?') {
            Some((r, q)) => (r, Some(q.to_string())),
            None => (rest, None),
        };
        let scheme_end = rest.find(':').filter(|&i| {
            i > 0
                && rest[..i]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        });
        let (scheme, authority, path) = match scheme_end {
            Some(i) => {
                let after = &rest[i + 1..];
                match after.strip_prefix("//") {
                    Some(hier) => match hier.find('/') {
                        Some(s) => (&rest[..i], &hier[..s], hier[s..].to_string()),
                        None => (&rest[..i], hier, "/".to_string()),
                    },
                    None => (&rest[..i], "", after.to_string()),
                }
            }
            None => ("", "", rest.to_string()),
        };
        Url {
            method,
            href: href.to_string(),
            scheme: scheme.to_string(),
            authority: authority.to_string(),
            path,
            query,
            fragment,
        }
    }

    /// The decoded `name=value` pairs of the query, in order.
    pub fn query_pairs(&self) -> Vec<(String, String)> {
        let Some(q) = &self.query else {
            return Vec::new();
        };
        q.split('&')
            .filter(|p| !p.is_empty())
            .map(|p| match p.split_once('=') {
                Some((k, v)) => (decode_component(k), decode_component(v)),
                None => (decode_component(p), String::new()),
            })
            .collect()
    }

    /// The first value of the query parameter `name`.
    pub fn query_param(&self, name: &str) -> Option<String> {
        self.query_pairs()
            .into_iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.href)
    }
}

/// Undoes `%XX` and `+` (a space) as used in queries and forms. Bad escapes stay as written.
pub fn decode_component(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() && hex(b[i + 1]).is_some() && hex(b[i + 2]).is_some() => {
                out.push(hex(b[i + 1]).unwrap() * 16 + hex(b[i + 2]).unwrap());
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    (c as char).to_digit(16).map(|d| d as u8)
}

/// The form encoding of one value: letters, digits and `-._~` stay, a space is `+`, every
/// other byte is `%XX`.
pub fn encode_component(s: &str) -> String {
    let mut out = String::new();
    for &c in s.as_bytes() {
        match c {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(c as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", c)),
        }
    }
    out
}
