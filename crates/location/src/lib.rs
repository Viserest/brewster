//! Where pages come from: URLs, resolving links against a page, and loading a page's text.
//!
//! `file://` and plain `http://` are supported using only `std`. `https://` needs TLS,
//! which `std` does not have, so it reports a clear error for now.

use std::{
    fmt,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    scheme: String,
    /// `host[:port]`; empty for `file:` and `about:`.
    authority: String,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

fn has_scheme(s: &str) -> bool {
    let Some(i) = s.find(':') else {
        return false;
    };
    let scheme = &s[..i];
    scheme.len() > 1
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}

/// Resolves `.` and `..` segments of an absolute path.
fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    let mut p = String::from("/");
    p.push_str(&out.join("/"));
    if path.ends_with('/') && p.len() > 1 {
        p.push('/');
    }
    p
}

impl Url {
    /// Parses an absolute URL (`scheme://authority/path`, or `about:name`).
    pub fn parse(s: &str) -> Result<Url, String> {
        let s = s.trim();
        if !has_scheme(s) {
            return Err(format!("`{}` is not an absolute URL", s));
        }
        let (rest, fragment) = match s.split_once('#') {
            Some((a, f)) => (a, Some(f.to_string())),
            None => (s, None),
        };
        let (rest, query) = match rest.split_once('?') {
            Some((a, q)) => (a, Some(q.to_string())),
            None => (rest, None),
        };
        let (scheme, after) = rest.split_once(':').expect("has_scheme checked the colon");
        let scheme = scheme.to_ascii_lowercase();
        if let Some(rest) = after.strip_prefix("//") {
            let (authority, path) = match rest.find('/') {
                Some(i) => (&rest[..i], &rest[i..]),
                None => (rest, ""),
            };
            if authority.is_empty() && scheme != "file" {
                return Err(format!("`{}` has no host", s));
            }
            let path = if path.is_empty() && scheme != "file" {
                "/".to_string()
            } else {
                normalize(path)
            };
            Ok(Url {
                scheme,
                authority: authority.to_ascii_lowercase(),
                path,
                query,
                fragment,
            })
        } else {
            Ok(Url {
                scheme,
                authority: String::new(),
                path: after.to_string(),
                query,
                fragment,
            })
        }
    }

    /// A `file:` URL for a file system path; relative paths are taken from the current
    /// directory.
    pub fn from_path(p: &str) -> Url {
        let path = if Path::new(p).is_absolute() {
            PathBuf::from(p)
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("/"))
                .join(p)
        };
        let s = path.to_string_lossy().replace('\\', "/");
        let abs = if s.starts_with('/') {
            s
        } else {
            format!("/{}", s)
        };
        Url {
            scheme: "file".to_string(),
            authority: String::new(),
            path: normalize(&abs),
            query: None,
            fragment: None,
        }
    }

    /// What a user typed: a URL when it has a scheme, else a file path.
    pub fn from_input(s: &str) -> Url {
        Url::parse(s).unwrap_or_else(|_| Url::from_path(s))
    }

    pub fn about(name: &str) -> Url {
        Url {
            scheme: "about".to_string(),
            authority: String::new(),
            path: name.to_string(),
            query: None,
            fragment: None,
        }
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// `host[:port]`, empty for local pages. Apps are keyed by this and the path.
    pub fn authority(&self) -> &str {
        &self.authority
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    pub fn is_file(&self) -> bool {
        self.scheme == "file"
    }

    pub fn to_file_path(&self) -> Option<PathBuf> {
        self.is_file().then(|| PathBuf::from(&self.path))
    }

    /// The last path segment, or the authority when there is none: a short name for a tab.
    pub fn short_name(&self) -> String {
        let seg = self
            .path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("");
        if !seg.is_empty() {
            seg.to_string()
        } else if !self.authority.is_empty() {
            self.authority.clone()
        } else {
            self.to_string()
        }
    }

    /// Resolves a link found on this page.
    pub fn resolve(&self, href: &str) -> Result<Url, String> {
        let href = href.trim();
        if href.is_empty() {
            return Err("empty link".to_string());
        }
        if has_scheme(href) {
            return Url::parse(href);
        }
        if let Some(rest) = href.strip_prefix("//") {
            return Url::parse(&format!("{}://{}", self.scheme, rest));
        }
        if self.scheme == "about" {
            return Ok(Url::from_path(href));
        }
        let (rest, fragment) = match href.split_once('#') {
            Some((a, f)) => (a, Some(f.to_string())),
            None => (href, None),
        };
        let (rest, query) = match rest.split_once('?') {
            Some((a, q)) => (a, Some(q.to_string())),
            None => (rest, None),
        };
        let mut u = self.clone();
        u.fragment = fragment;
        if rest.is_empty() {
            // `#frag` or `?query` alone: same document.
            if query.is_some() {
                u.query = query;
            }
            return Ok(u);
        }
        u.query = query;
        u.path = if rest.starts_with('/') {
            normalize(rest)
        } else {
            let dir = match self.path.rfind('/') {
                Some(i) => &self.path[..=i],
                None => "/",
            };
            normalize(&format!("{}{}", dir, rest))
        };
        Ok(u)
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scheme == "about" {
            write!(f, "about:{}", self.path)?;
        } else {
            write!(f, "{}://{}{}", self.scheme, self.authority, self.path)?;
        }
        if let Some(q) = &self.query {
            write!(f, "?{}", q)?;
        }
        if let Some(fr) = &self.fragment {
            write!(f, "#{}", fr)?;
        }
        Ok(())
    }
}

/// Loads the text of a page.
pub trait Loader {
    fn load(&self, url: &Url) -> Result<String, String>;

    /// The raw bytes behind a URL (compiled plugins are binary). Loaders that only know
    /// text can leave the default.
    fn load_bytes(&self, url: &Url) -> Result<Vec<u8>, String> {
        self.load(url).map(String::into_bytes)
    }
}

/// `file:` from disk and `http:` over a plain TCP connection.
#[derive(Debug, Clone)]
pub struct DefaultLoader {
    pub timeout: Duration,
}

impl Default for DefaultLoader {
    fn default() -> DefaultLoader {
        DefaultLoader {
            timeout: Duration::from_secs(10),
        }
    }
}

const MAX_BODY: usize = 8 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

impl Loader for DefaultLoader {
    fn load(&self, url: &Url) -> Result<String, String> {
        let bytes = self.load_bytes(url)?;
        String::from_utf8(bytes).map_err(|_| format!("{} is not valid UTF-8", url))
    }

    fn load_bytes(&self, url: &Url) -> Result<Vec<u8>, String> {
        match url.scheme() {
            "file" => {
                let p = url.to_file_path().expect("file url has a path");
                std::fs::read(&p).map_err(|e| format!("cannot read {}: {}", p.display(), e))
            }
            "http" => self.http_get(url, 0),
            "https" => Err(
                "https is not supported yet (std has no TLS); use http:// or a file".to_string(),
            ),
            s => Err(format!("cannot load `{}` URLs", s)),
        }
    }
}

impl DefaultLoader {
    fn http_get(&self, url: &Url, redirects: usize) -> Result<Vec<u8>, String> {
        let (host, port) = match url.authority().rsplit_once(':') {
            Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (
                h.to_string(),
                p.parse::<u16>()
                    .map_err(|_| format!("bad port in {}", url))?,
            ),
            _ => (url.authority().to_string(), 80),
        };
        let addr = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| format!("cannot resolve {}: {}", host, e))?
            .next()
            .ok_or_else(|| format!("cannot resolve {}", host))?;
        let mut stream = TcpStream::connect_timeout(&addr, self.timeout)
            .map_err(|e| format!("cannot connect to {}: {}", url.authority(), e))?;
        stream.set_read_timeout(Some(self.timeout)).ok();
        stream.set_write_timeout(Some(self.timeout)).ok();
        let mut target = url.path().to_string();
        if let Some(q) = url.query() {
            target.push('?');
            target.push_str(q);
        }
        let req = format!(
            "GET {} HTTP/1.0\r\nHost: {}\r\nUser-Agent: brewster\r\nAccept: text/plain, */*\r\nConnection: close\r\n\r\n",
            target,
            url.authority()
        );
        stream
            .write_all(req.as_bytes())
            .map_err(|e| e.to_string())?;
        let mut raw = Vec::new();
        stream
            .take(MAX_BODY as u64 + 65536)
            .read_to_end(&mut raw)
            .map_err(|e| format!("reading {}: {}", url, e))?;
        let (status, headers, body) = split_response(&raw)?;
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let loc = headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("location"))
                .map(|(_, v)| v.clone())
                .ok_or_else(|| format!("HTTP {} without a Location", status))?;
            if redirects >= MAX_REDIRECTS {
                return Err("too many redirects".to_string());
            }
            let next = url.resolve(&loc)?;
            return match next.scheme() {
                "http" => self.http_get(&next, redirects + 1),
                _ => self.load_bytes(&next),
            };
        }
        if !(200..300).contains(&status) {
            return Err(format!("HTTP {} from {}", status, url));
        }
        let chunked = headers.iter().any(|(k, v)| {
            k.eq_ignore_ascii_case("transfer-encoding")
                && v.to_ascii_lowercase().contains("chunked")
        });
        let body = if chunked {
            dechunk(body)?
        } else {
            body.to_vec()
        };
        if body.len() > MAX_BODY {
            return Err("page is larger than 8 MiB".to_string());
        }
        Ok(body)
    }
}

/// `(status, headers, body)` of a raw HTTP response.
fn split_response(raw: &[u8]) -> Result<(u16, Vec<(String, String)>, &[u8]), String> {
    let end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("malformed HTTP response")?;
    let head = std::str::from_utf8(&raw[..end]).map_err(|_| "malformed HTTP headers")?;
    let mut lines = head.split("\r\n");
    let status_line = lines.next().ok_or("malformed HTTP response")?;
    let mut parts = status_line.split_whitespace();
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/") {
        return Err("not an HTTP response".to_string());
    }
    let status: u16 = parts
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or("malformed HTTP status line")?;
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    Ok((status, headers, &raw[end + 4..]))
}

fn dechunk(mut body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let eol = body
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("malformed chunked body")?;
        let size_str = std::str::from_utf8(&body[..eol]).map_err(|_| "malformed chunk size")?;
        let size_str = size_str.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16).map_err(|_| "malformed chunk size")?;
        body = &body[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if body.len() < size + 2 {
            return Err("truncated chunked body".to_string());
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::thread;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn parses_http_urls() {
        let x = u("HTTP://Example.COM:8080/a/b.rs?x=1#top");
        assert_eq!(x.scheme(), "http");
        assert_eq!(x.authority(), "example.com:8080");
        assert_eq!(x.path(), "/a/b.rs");
        assert_eq!(x.query(), Some("x=1"));
        assert_eq!(x.fragment(), Some("top"));
        assert_eq!(x.to_string(), "http://example.com:8080/a/b.rs?x=1#top");
        assert_eq!(u("http://example.com").path(), "/");
    }

    #[test]
    fn parses_file_and_about() {
        let f = u("file:///home/me/a.rs");
        assert!(f.is_file());
        assert_eq!(f.authority(), "");
        assert_eq!(f.to_file_path(), Some(PathBuf::from("/home/me/a.rs")));
        assert_eq!(f.to_string(), "file:///home/me/a.rs");
        let a = u("about:newtab");
        assert_eq!(a.scheme(), "about");
        assert_eq!(a.to_string(), "about:newtab");
    }

    #[test]
    fn rejects_non_urls() {
        assert!(Url::parse("examples/demo.rs").is_err());
        assert!(Url::parse("/abs/path").is_err());
        assert!(Url::parse("http://").is_err());
        assert!(Url::parse("x:y").is_err()); // one-letter schemes are drive letters
    }

    #[test]
    fn from_input_falls_back_to_paths() {
        assert_eq!(Url::from_input("http://a.b/c").authority(), "a.b");
        let f = Url::from_input("/tmp/../tmp/x.rs");
        assert_eq!(f.to_string(), "file:///tmp/x.rs");
        assert!(Url::from_input("rel/x.rs").path().ends_with("/rel/x.rs"));
    }

    #[test]
    fn resolves_relative_links() {
        let base = u("http://a.b/dir/page.rs?q=1");
        assert_eq!(
            base.resolve("other.rs").unwrap().to_string(),
            "http://a.b/dir/other.rs"
        );
        assert_eq!(
            base.resolve("../x.rs").unwrap().to_string(),
            "http://a.b/x.rs"
        );
        assert_eq!(
            base.resolve("/root.rs").unwrap().to_string(),
            "http://a.b/root.rs"
        );
        assert_eq!(
            base.resolve("sub/").unwrap().to_string(),
            "http://a.b/dir/sub/"
        );
        assert_eq!(
            base.resolve("#frag").unwrap().to_string(),
            "http://a.b/dir/page.rs?q=1#frag"
        );
        assert_eq!(base.resolve("//c.d/z").unwrap().to_string(), "http://c.d/z");
        assert_eq!(
            base.resolve("https://e.f/g").unwrap().to_string(),
            "https://e.f/g"
        );
        assert_eq!(base.resolve("../../../up.rs").unwrap().path(), "/up.rs");
        assert!(base.resolve("  ").is_err());
    }

    #[test]
    fn resolves_against_files_and_about() {
        let f = u("file:///home/me/site/index.rs");
        assert_eq!(
            f.resolve("a/b.rs").unwrap().to_string(),
            "file:///home/me/site/a/b.rs"
        );
        assert_eq!(
            f.resolve("../c.rs").unwrap().to_string(),
            "file:///home/me/c.rs"
        );
        let a = u("about:newtab");
        assert!(a.resolve("x.rs").unwrap().is_file());
    }

    #[test]
    fn short_names() {
        assert_eq!(u("file:///a/b/c.rs").short_name(), "c.rs");
        assert_eq!(u("http://host.org/").short_name(), "host.org");
        assert_eq!(u("http://host.org/dir/").short_name(), "dir");
        assert_eq!(u("about:newtab").short_name(), "newtab");
    }

    #[test]
    fn loads_files() {
        let dir = std::env::temp_dir().join(format!("brewster-loc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.rs");
        std::fs::write(&p, "p \"x\"").unwrap();
        let l = DefaultLoader::default();
        assert_eq!(
            l.load(&Url::from_path(p.to_str().unwrap())).unwrap(),
            "p \"x\""
        );
        let missing = l.load(&Url::from_path(dir.join("nope.rs").to_str().unwrap()));
        assert!(missing.unwrap_err().starts_with("cannot read"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_schemes_explain_themselves() {
        let l = DefaultLoader::default();
        assert!(l.load(&u("https://a.b/")).unwrap_err().contains("TLS"));
        assert!(l.load(&u("ftp://a.b/")).unwrap_err().contains("ftp"));
    }

    /// Serves `responses` in order, one per connection, on a free local port.
    fn serve(responses: Vec<Vec<u8>>) -> (u16, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let h = thread::spawn(move || {
            let mut seen = Vec::new();
            for r in responses {
                let (mut s, _) = listener.accept().unwrap();
                let mut line = String::new();
                {
                    let mut br = BufReader::new(&s);
                    br.read_line(&mut line).unwrap();
                    let mut l = String::new();
                    while br.read_line(&mut l).unwrap() > 2 {
                        l.clear();
                    }
                }
                seen.push(line.trim().to_string());
                s.write_all(&r).unwrap();
            }
            seen
        });
        (port, h)
    }

    #[test]
    fn http_get_reads_the_body() {
        let (port, h) = serve(vec![
            b"HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\n\r\np \"hi\"".to_vec(),
        ]);
        let url = u(&format!("http://127.0.0.1:{}/a/b.rs?x=1", port));
        assert_eq!(DefaultLoader::default().load(&url).unwrap(), "p \"hi\"");
        assert_eq!(
            h.join().unwrap(),
            vec!["GET /a/b.rs?x=1 HTTP/1.0".to_string()]
        );
    }

    #[test]
    fn http_get_follows_redirects_and_decodes_chunks() {
        let (port, h) = serve(vec![
            b"HTTP/1.1 302 Found\r\nLocation: /real.rs\r\n\r\n".to_vec(),
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\np \"\r\n3;ext=1\r\nab\"\r\n0\r\n\r\n".to_vec(),
        ]);
        let url = u(&format!("http://127.0.0.1:{}/old.rs", port));
        assert_eq!(DefaultLoader::default().load(&url).unwrap(), "p \"ab\"");
        assert_eq!(h.join().unwrap().len(), 2);
    }

    #[test]
    fn http_errors_are_reported() {
        let (port, h) = serve(vec![b"HTTP/1.0 404 Not Found\r\n\r\nnope".to_vec()]);
        let url = u(&format!("http://127.0.0.1:{}/x", port));
        assert!(
            DefaultLoader::default()
                .load(&url)
                .unwrap_err()
                .contains("HTTP 404")
        );
        h.join().unwrap();
        // Nothing is listening any more.
        assert!(
            DefaultLoader::default()
                .load(&url)
                .unwrap_err()
                .contains("cannot connect")
        );
    }

    #[test]
    fn dechunk_rejects_garbage() {
        assert!(dechunk(b"zz\r\nabc").is_err());
        assert!(dechunk(b"5\r\nab").is_err());
        assert_eq!(dechunk(b"0\r\n\r\n").unwrap(), Vec::<u8>::new());
    }
}
