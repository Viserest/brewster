//! Brewster Project
//!
//! This file includes all pages

use cre::*;

cre::router!([
    get "/": index,
    get "/examples": examples,
]);

pub fn index() -> Element {
    bx!({ extends: &body() }, [
        h2!("What"),
        h2!("How"),
        h2!("Why"),
    ])
}

pub fn examples() -> Element {
    // One page which shows an h1 with text "Hello World"
    let hello_world = bx!([
        h3!("Hello World Page"),
        pre!(
            r#"rust
use cre::*;
cre::page!(page);
pub fn page() -> Element {
    h1!("Hello World")
}"#
        ),
    ]);

    // Shows how a before, after, and extends works.
    //
    // Box A:
    // - No children.
    // - Before attribute with an element.
    // - After attribute with an element.
    //
    // Box B extends Box A:
    // - Prepends Box A's before attribute to its children.
    // - Appends Box A's after attribute to its children.
    let before_after_extends = bx!([
        h3!("Before, After, and Extends"),
        pre!(
            r#"rust
use cre::*;
cre::page!(page);

fn body() -> Element {
    bx!({
        before: bx!([ link!("/", "Index"), link!("/about", "About") ]),
        after: p!("Copyright &copy; 2026"),
    })
}

pub fn page() -> Element {
    bx!({ extends: &body() }, [
        h1!("Interesting Content"),
        h2!("Content 1"),
        h3!("Sub-Content 1.1"),
        p!("Really important stuff"),
        h3!("Sub-Content 1.2"),
        h2!("Content 2"),
        h3!("Sub-Content 2.1"),
        p!("Not as important stuff"),
    ])
}"#
        ),
    ]);

    let routers = bx!([
        h3!("Router"),
        pre!(
            r#"rust
use cre::*;

/// Create router using ```method "path": function```
cre::router([
    get "/": index,
    get "/hello/:name": name,
    get "/health": health,
]);

pub fn index() -> Element {
    h1!("Hello World")
}

/// Name path handler which gets name from url path params
pub fn name() -> Element {
    let params = root().url().params();
    let your_name = params.get("name");
    h1!("Hello {}", your_name)
}

pub fn health() -> Element {
    p!("OK")
}"#
        ),
    ]);

    let forms = bx!([
        h3!("Forms"),
        pre!(
            r#"rust
use cre::*;

// Generates
// 1. struct Login {...}
// 2. Login::element() -> Element;
// 3. Login::call(this: &Element);
cre::form!(
    struct Login {
        username: String => Text,
        password: String => Password,
    }
    post "/login";
);

cre::page!(login);

pub fn login() -> Element {
    bx!([
        p!({ id: "msg" }),
        Login::element(),
        button!({ onclick: Login::call }, "Login"),
    ])
}"#
        ),
    ]);

    bx!({ extends: &body() }, [
        h2!("Examples"),
        hello_world,
        before_after_extends,
        routers,
        forms,
    ])
}

pub fn body() -> Element {
    bx!({
        before: bx!({ align: "middle,unset"}, [
            bx!([h1!("Brewster Project")]),
            bx!([link!("/", "Index"), link!("/", "Examples")])
        ]),
        after: bx!([
            p!("Copyright &copy; Brewster Project"),
            p!("Authored by Seth Cooper"),
        ])
    })
}
