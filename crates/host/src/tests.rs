use super::*;

use cre::*;

fn page_a() -> Element {
    bx!([
        p!({ id: "out" }, "a"),
        button!({ onclick: |this: &Element, root: &Root| {
            this.set_text("pressed");
            root.tree.query("#out").unwrap().set_text("changed");
        } }, "press"),
    ])
}

fn plugin_b() -> Element {
    bx!([button!({ onclick: |this: &Element, _: &Root| this.set_text("plugin pressed") }, "b")])
}

fn broken() -> Element {
    // Children on a leaf are a recorded problem, not a crash.
    let leaf = p!("x");
    leaf.push(p!("y"));
    bx!([leaf])
}

fn texts(es: &[tree::Element], out: &mut Vec<String>) {
    for e in es {
        if let Some(t) = e.args.first() {
            out.push(t.clone());
        }
        texts(&e.children, out);
    }
}

fn all_text(s: &Session) -> Vec<String> {
    let mut v = Vec::new();
    texts(s.tree(), &mut v);
    v
}

fn id_of(s: &Session, text: &str) -> usize {
    fn go(es: &[tree::Element], text: &str) -> Option<usize> {
        es.iter().find_map(|e| {
            if e.args.first().map(String::as_str) == Some(text) {
                Some(e.id)
            } else {
                go(&e.children, text)
            }
        })
    }
    go(s.tree(), text).unwrap()
}

#[test]
fn page_and_plugins_share_a_tree_with_distinct_ids() {
    let mut s = Session::new();
    s.set_page(Box::new(NativeGuest::new(page_a)));
    s.add_plugin("b", Box::new(NativeGuest::new(plugin_b)));
    s.refresh().unwrap();
    assert_eq!(all_text(&s), ["a", "press", "b"]);
    let mut ids = Vec::new();
    fn collect(es: &[tree::Element], ids: &mut Vec<usize>) {
        for e in es {
            ids.push(e.id);
            collect(&e.children, ids);
        }
    }
    collect(s.tree(), &mut ids);
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n);
}

#[test]
fn a_click_reaches_the_guest_that_owns_the_element() {
    let mut s = Session::new();
    s.set_page(Box::new(NativeGuest::new(page_a)));
    s.add_plugin("b", Box::new(NativeGuest::new(plugin_b)));
    s.refresh().unwrap();
    let press = id_of(&s, "press");
    let b = id_of(&s, "b");
    assert!(s.is_clickable(press) && s.is_clickable(b));
    assert!(!s.is_clickable(id_of(&s, "a")));
    assert_eq!(s.click(b), Ok(true));
    assert_eq!(all_text(&s), ["a", "press", "plugin pressed"]);
    assert_eq!(s.click(press), Ok(true));
    // `this` was the button, and the handler reached another element through `root`.
    assert_eq!(all_text(&s), ["changed", "pressed", "plugin pressed"]);
    assert_eq!(s.click(id_of(&s, "changed")), Ok(false));
    assert_eq!(s.click(usize::MAX), Ok(false));
}

#[test]
fn problems_become_diagnostics() {
    let mut s = Session::new();
    s.set_page(Box::new(NativeGuest::new(page_a)));
    s.add_plugin("bad", Box::new(NativeGuest::new(broken)));
    s.refresh().unwrap();
    assert_eq!(s.diagnostics.len(), 1, "{:?}", s.diagnostics);
    assert!(s.diagnostics[0].starts_with("plugin bad: "), "{:?}", s.diagnostics);
    assert!(s.diagnostics[0].contains("cannot have children"));
}

struct Failing;
impl Guest for Failing {
    fn tree(&mut self) -> Result<Vec<tree::Element>, String> {
        Err("trapped".into())
    }
    fn click(&mut self, _: usize) -> Result<bool, String> {
        Err("trapped".into())
    }
    fn problems(&mut self) -> Vec<String> {
        vec![]
    }
}

#[test]
fn a_failing_plugin_is_left_out_but_a_failing_page_is_an_error() {
    let mut s = Session::new();
    s.set_page(Box::new(NativeGuest::new(page_a)));
    s.add_plugin("x", Box::new(Failing));
    s.refresh().unwrap();
    assert_eq!(s.diagnostics, ["plugin x: trapped"]);
    assert_eq!(all_text(&s), ["a", "press"]);

    let mut s = Session::new();
    s.set_page(Box::new(Failing));
    assert_eq!(s.refresh(), Err("trapped".to_string()));
}

#[test]
fn map_runner_runs_known_modules_only() {
    let r = MapRunner::new().page("a", page_a);
    assert!(r.instantiate(&MapRunner::module("a")).is_ok());
    assert!(r.instantiate(&MapRunner::module("b")).is_err());
    assert!(plugin::is_wasm(&MapRunner::module("a")));
    assert!(Unavailable.instantiate(b"\0asm").is_err());
}
