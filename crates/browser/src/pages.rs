//! Pages the browser writes itself: the new-tab page and the error page. They are built
//! with the same SDK as any page.

use cre::*;

pub fn new_tab(apps: &[String], hints: &[String]) -> Element {
    let e = bx!([
        h1!("brewster"),
        p!("A browser for Rust pages."),
        p!(""),
    ]);
    for h in hints {
        e.push(p!(h.replace(['\n', '\r'], " ")));
    }
    e.push(p!(""));
    e.push(link!("about:settings", "Open settings"));
    e.push(p!(""));
    e.push(h3!("Applications"));
    if apps.is_empty() {
        e.push(p!("none installed"));
    }
    for a in apps {
        e.push(p!(format!("- {}", a)));
    }
    e
}

pub fn error(url: &str, message: &str) -> Element {
    bx!([
        h2!("Cannot open this page"),
        p!(url.replace(['\n', '\r'], " ")),
        p!(""),
        p!({ fcolor: "red" }, message.replace(['\n', '\r'], " ")),
        p!(""),
        link!("about:newtab", "Go to the new tab page"),
    ])
}
