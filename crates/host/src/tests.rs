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

// ── the wasm protocol, over a fake engine ───────────────────────────

/// What an engine would give: a linear memory and the four exports. The page itself runs
/// natively here, so the bytes crossing the "boundary" are exactly what a module sends.
struct Fake {
    page: cre::LivePage,
    memory: Vec<u8>,
    version: u32,
}

const BASE: usize = 4096; // where outputs are placed, like an allocator would

impl Fake {
    fn new(build: fn() -> Element) -> Fake {
        Fake { page: cre::LivePage::new(build), memory: vec![0; BASE], version: cre::abi::VERSION }
    }
    fn put(&mut self, bytes: Vec<u8>) -> u64 {
        self.memory.truncate(BASE);
        let len = bytes.len();
        self.memory.extend(bytes);
        ((BASE as u64) << 32) | len as u64
    }
}

impl Instance for Fake {
    fn abi(&mut self) -> Result<u32, String> {
        Ok(self.version)
    }
    fn page(&mut self) -> Result<u64, String> {
        let b = tree::wire::encode(&self.page.tree());
        Ok(self.put(b))
    }
    fn event(&mut self, id: u32) -> Result<u32, String> {
        Ok(self.page.click(id as usize) as u32)
    }
    fn problems(&mut self) -> Result<u64, String> {
        let t = self.page.problems().join("\n");
        Ok(self.put(t.into_bytes()))
    }
    fn read(&mut self, ptr: u32, len: u32) -> Result<Vec<u8>, String> {
        let (a, b) = (ptr as usize, ptr as usize + len as usize);
        self.memory.get(a..b).map(<[u8]>::to_vec).ok_or_else(|| "out of bounds".to_string())
    }
}

#[test]
fn a_wasm_guest_reads_the_tree_and_runs_clicks() {
    let g = WasmGuest::new(Fake::new(page_a)).unwrap();
    let mut s = Session::new();
    s.set_page(Box::new(g));
    s.refresh().unwrap();
    assert_eq!(all_text(&s), ["a", "press"]);
    let press = id_of(&s, "press");
    assert!(s.is_clickable(press));
    assert_eq!(s.click(press), Ok(true));
    assert_eq!(all_text(&s), ["changed", "pressed"]);
    assert_eq!(s.click(usize::MAX), Ok(false));
}

#[test]
fn a_wasm_guest_reports_problems_and_refuses_other_versions() {
    let mut g = WasmGuest::new(Fake::new(broken)).unwrap();
    g.tree().unwrap();
    let p = g.problems();
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].contains("cannot have children"));

    let mut f = Fake::new(page_a);
    f.version = 99;
    let e = WasmGuest::new(f).err().unwrap();
    assert!(e.contains("version 99"), "{e}");
}

#[test]
fn a_wasm_guest_survives_bad_output() {
    struct Bad(u64);
    impl Instance for Bad {
        fn abi(&mut self) -> Result<u32, String> { Ok(cre::abi::VERSION) }
        fn page(&mut self) -> Result<u64, String> { Ok(self.0) }
        fn event(&mut self, _: u32) -> Result<u32, String> { Err("trap".into()) }
        fn problems(&mut self) -> Result<u64, String> { Ok(self.0) }
        fn read(&mut self, _: u32, _: u32) -> Result<Vec<u8>, String> { Ok(vec![1, 2, 3]) }
    }
    // Bytes that are not a tree: an error, not a panic.
    let mut g = WasmGuest::new(Bad(3)).unwrap();
    assert!(g.tree().unwrap_err().starts_with("bad page from module"));
    // A length over the limit is refused before anything is read.
    let mut g = WasmGuest::new(Bad(u64::from(MAX_OUTPUT) + 1)).unwrap();
    assert!(g.tree().unwrap_err().contains("more than the limit"));
    assert_eq!(g.problems().len(), 1);
    // A trap in a handler is an error for the caller.
    assert_eq!(g.click(1), Err("trap".to_string()));
    // An id no u32 can hold has no handler.
    assert_eq!(g.click(usize::MAX), Ok(false));
}

#[test]
fn a_module_without_the_exports_is_not_a_page() {
    struct Empty;
    impl Instance for Empty {
        fn abi(&mut self) -> Result<u32, String> { Err("no export `cre_abi`".into()) }
        fn page(&mut self) -> Result<u64, String> { unreachable!() }
        fn event(&mut self, _: u32) -> Result<u32, String> { unreachable!() }
        fn problems(&mut self) -> Result<u64, String> { unreachable!() }
        fn read(&mut self, _: u32, _: u32) -> Result<Vec<u8>, String> { unreachable!() }
    }
    let e = WasmGuest::new(Empty).err().unwrap();
    assert!(e.starts_with("not a cre page module"), "{e}");
}
