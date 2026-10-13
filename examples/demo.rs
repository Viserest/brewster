//! The demo page. Build it as a module with
//! `rustc --edition 2024 --target wasm32-unknown-unknown --crate-type cdylib ...`
//! (the browser does this itself when it is given this file).

use cre::*;

cre::page!(page);

pub fn page() -> Element {
    // A template: its own children are skipped when extended, `before` / `after` are kept.
    let body = bx!({
        bcolor: "#181818",
        fcolor: "white",
        pad: "1,0,1,2",
        before: bx!([
            link!("https://example.com", "Home Page"),
            link!("https://example.com/projects", "Projects Page"),
        ]),
        after: bx!([p!("Copyright &copy; 2026")]),
    });

    bx!({ extends: &body }, [
        h1!({ align_x: "middle" }, "Hello, pseudo"),
        h2!("Second header"),
        h3!("Third header"),
        h4!("Fourth header"),
        h5!("Fifth header"),
        h6!("Sixth header"),
        p!({ fcolor: "yellow" }, "Yellow text"),
        p!({ bcolor: "black", fcolor: "white" }, "Black background, white foreground"),
        pre!(r#"

    Something cray

fn main() {
  print("raw strings keep    spacing")
}"#),
        bx!({ border: "1", border_color: "cyan" }, [
            h3!("Admin Login"),
            input!({ size: "20,min" }, Text, "name", "John Doe the third of his name"),
            input!(Password, "password", "hunter2"),
            button!(
                { onclick: |_this: &Element, root: &Root| greet(root, "friend") },
                "Submit this"
            ),
        ]),
        p!({ fcolor: "green", id: "out" }, "Press the button (enter or click) to run its function."),
        link!("banner.rs", "A page that is a plugin"),
    ])
}

fn greet(root: &Root, who: &str) {
    if let Some(out) = root.tree.query("#out") {
        out.set_text(format!("Hello, {who}!"));
    }
}
