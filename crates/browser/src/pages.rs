//! Pages the browser writes itself: the new-tab page and the error page.

/// A string literal that needs no escaping: a raw string with enough `#`s. Newlines become
/// spaces (a page line is one paragraph here).
pub fn raw(s: &str) -> String {
    let s = s.replace(['\n', '\r'], " ");
    let mut n = 1;
    while s.contains(&format!("\"{}", "#".repeat(n))) {
        n += 1;
    }
    let h = "#".repeat(n);
    format!("r{h}\"{s}\"{h}")
}

pub fn new_tab(apps: &[String], hints: &[String]) -> String {
    let mut s = String::new();
    s.push_str(&format!("h1 {}\n", raw("brewster")));
    s.push_str(&format!("p {}\n", raw("A browser for .cre pages.")));
    s.push_str("p \"\"\n");
    for h in hints {
        s.push_str(&format!("p {}\n", raw(h)));
    }
    s.push_str("p \"\"\n");
    s.push_str(&format!("link {} {}\n", raw("about:settings"), raw("Open settings")));
    s.push_str("p \"\"\n");
    s.push_str(&format!("h3 {}\n", raw("Applications")));
    if apps.is_empty() {
        s.push_str(&format!("p {}\n", raw("none installed")));
    }
    for a in apps {
        s.push_str(&format!("p {}\n", raw(&format!("- {}", a))));
    }
    s
}

pub fn error(url: &str, message: &str) -> String {
    let mut s = String::new();
    s.push_str(&format!("h2 {}\n", raw("Cannot open this page")));
    s.push_str(&format!("p {}\n", raw(url)));
    s.push_str("p \"\"\n");
    s.push_str(&format!("p.fg:red {}\n", raw(message)));
    s.push_str("p \"\"\n");
    s.push_str(&format!("link {} {}\n", raw("about:newtab"), raw("Go to the new tab page")));
    s
}
