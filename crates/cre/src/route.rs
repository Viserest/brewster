//! Routing by the page's address: what [`router!`](crate::router) expands to.

use crate::{Element, Method, Url, decode_component};

/// What a route handler is given: the address and what the pattern captured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub url: Url,
    /// `(name, value)` for each `:name` (and `*name`) of the matched pattern, decoded.
    pub params: Vec<(String, String)>,
}

impl Request {
    pub fn method(&self) -> Method {
        self.url.method
    }

    pub fn path(&self) -> &str {
        &self.url.path
    }

    /// What the pattern captured for `name`.
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// A query parameter (`?name=value`), decoded.
    pub fn query(&self, name: &str) -> Option<String> {
        self.url.query_param(name)
    }
}

/// Matches `url` against `pattern` for `method`. Segments are separated by `/`; a literal
/// segment must be equal, `:name` takes any one segment, and a last segment `*name` takes
/// the rest of the path. A trailing `/` makes no difference.
pub fn matches(method: Method, pattern: &str, url: &Url) -> Option<Vec<(String, String)>> {
    if url.method != method {
        return None;
    }
    let want: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let have: Vec<&str> = url.path.split('/').filter(|s| !s.is_empty()).collect();
    let mut params = Vec::new();
    for (i, w) in want.iter().enumerate() {
        if let Some(name) = w.strip_prefix('*') {
            if i + 1 != want.len() {
                return None; // a wildcard is only allowed last
            }
            let rest = have.get(i..).unwrap_or(&[]).join("/");
            params.push((name.to_string(), decode_component(&rest)));
            return Some(params);
        }
        let h = have.get(i)?;
        match w.strip_prefix(':') {
            Some(name) => params.push((name.to_string(), decode_component(h))),
            None if w == h => {}
            None => return None,
        }
    }
    (want.len() == have.len()).then_some(params)
}

/// Marks a handler `fn() -> Element`.
pub struct NoArgs;
/// Marks a handler `fn(&Request) -> Element`.
pub struct WithRequest;

/// A route handler: with or without the request.
pub trait RouteFn<M> {
    fn call(&self, req: &Request) -> Element;
}

impl<F: Fn() -> Element> RouteFn<NoArgs> for F {
    fn call(&self, _: &Request) -> Element {
        self()
    }
}

impl<F: Fn(&Request) -> Element> RouteFn<WithRequest> for F {
    fn call(&self, req: &Request) -> Element {
        self(req)
    }
}

pub fn call<M, F: RouteFn<M>>(f: F, req: &Request) -> Element {
    f.call(req)
}

/// The page for an address no route matches.
pub fn not_found(url: &Url) -> Element {
    crate::bx!([
        crate::h2!("Not found"),
        crate::p!(format!("{} {} matches no route.", url.method, url.path)),
    ])
}
