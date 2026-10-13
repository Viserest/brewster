//! A plugin: a page module like any other. Installed for an application, it runs on every
//! page under it and its elements are added after the page's own.

use cre::*;

cre::page!(page);

pub fn page() -> Element {
    bx!([p!({ fcolor: "yellow" }, "Hello from a plugin")])
}
