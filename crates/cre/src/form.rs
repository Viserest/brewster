//! Helpers behind [`form!`](crate::form).

use crate::{Method, NetResult, ResponseFuture, encode_component, root};

/// A form field's value as text and back. Text that does not parse gives the type's
/// default, as an empty box would.
pub trait FormValue {
    fn to_form(&self) -> String;
    fn from_form(s: &str) -> Self;
}

impl FormValue for String {
    fn to_form(&self) -> String {
        self.clone()
    }
    fn from_form(s: &str) -> String {
        s.to_string()
    }
}

impl FormValue for bool {
    fn to_form(&self) -> String {
        self.to_string()
    }
    fn from_form(s: &str) -> bool {
        matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1" | "on" | "yes")
    }
}

macro_rules! number {
    ($($t:ty),*) => {$(
        impl FormValue for $t {
            fn to_form(&self) -> String { self.to_string() }
            fn from_form(s: &str) -> $t { s.trim().parse().unwrap_or_default() }
        }
    )*};
}
number!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize, f32, f64);

/// `name=value&name=value`, form-encoded.
pub fn urlencode(pairs: &[(&str, String)]) -> Vec<u8> {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v)))
        .collect::<Vec<_>>()
        .join("&")
        .into_bytes()
}

/// Sends a form: in the query for `GET`, `HEAD` and `OPTIONS`, else as the body.
pub fn send(method: Method, url: &str, pairs: &[(&str, String)]) -> ResponseFuture {
    let body = urlencode(pairs);
    let net = root().net();
    match method {
        Method::Get | Method::Head | Method::Options => {
            let target = if body.is_empty() {
                url.to_string()
            } else {
                let sep = if url.contains('?') { '&' } else { '?' };
                format!("{}{}{}", url, sep, String::from_utf8_lossy(&body))
            };
            net.send(method, &target, Vec::new(), Vec::new())
        }
        _ => net.send(
            method,
            url,
            vec![(
                "Content-Type".to_string(),
                "application/x-www-form-urlencoded".to_string(),
            )],
            body,
        ),
    }
}

/// One line for a form's status text.
pub fn describe(result: &NetResult) -> String {
    match result {
        Ok(r) if r.ok() => format!("done ({})", r.status),
        Ok(r) => format!("failed: HTTP {}", r.status),
        Err(e) => format!("failed: {}", e),
    }
}
