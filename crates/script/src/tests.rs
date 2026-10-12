use super::*;

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use location::{Loader, Url};
use plugin::{Plugin, Registry};
use tree::Node;

#[derive(Default)]
struct MapLoader(RefCell<HashMap<String, Vec<u8>>>);

impl MapLoader {
    fn set(&self, url: &str, body: &[u8]) {
        self.0.borrow_mut().insert(url.to_string(), body.to_vec());
    }
}

impl Loader for MapLoader {
    fn load(&self, url: &Url) -> Result<String, String> {
        self.load_bytes(url)
            .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
    }
    fn load_bytes(&self, url: &Url) -> Result<Vec<u8>, String> {
        self.0
            .borrow()
            .get(&url.to_string())
            .cloned()
            .ok_or_else(|| "offline".to_string())
    }
}

struct Env {
    registry: Rc<RefCell<Registry>>,
    loader: Rc<MapLoader>,
}

fn env() -> Env {
    Env {
        registry: Rc::new(RefCell::new(Registry::in_memory())),
        loader: Rc::new(MapLoader::default()),
    }
}

fn load_at(env: &Env, url: &str, src: &str) -> Result<Page, String> {
    Page::load(
        src,
        Url::parse(url).unwrap(),
        env.registry.clone(),
        env.loader.clone(),
    )
}

fn load(src: &str) -> Page {
    let e = env();
    load_at(&e, "http://site.test/", src).unwrap_or_else(|er| panic!("{}\n{}", er, src))
}

fn fail(src: &str) -> String {
    let e = env();
    match load_at(&e, "http://site.test/", src) {
        Ok(_) => panic!("expected an error for:\n{}", src),
        Err(er) => er,
    }
}

/// `tag: text` lines of the whole tree, indented by depth.
fn dump(n: &Node, depth: usize, out: &mut Vec<String>) {
    for c in n.children() {
        let mut args = c.args().join("|");
        if args.is_empty() {
            args = String::new();
        }
        out.push(
            format!("{}{} {}", "  ".repeat(depth), c.tag(), args)
                .trim_end()
                .to_string(),
        );
        dump(&c, depth + 1, out);
    }
}

fn shape(p: &Page) -> Vec<String> {
    let mut v = Vec::new();
    dump(p.root(), 0, &mut v);
    v
}

fn text_of(p: &Page, sel: &str) -> String {
    p.root().query(sel).unwrap().text()
}

#[test]
fn elements_build_a_tree() {
    let p = load("box.pad:1 {\n  h1 \"T\"\n  p \"a\"\n}\np \"b\"");
    assert_eq!(shape(&p), vec!["box", "  h1 T", "  p a", "p b"]);
    assert_eq!(
        p.root().children()[0].style_get("pad"),
        Some(vec!["1".to_string()])
    );
}

#[test]
fn numbers_and_expressions_in_arguments() {
    let p = load("p 5\np \"a\" + 1 + 2\np 1 + 2");
    assert_eq!(shape(&p), vec!["p 5", "p a12", "p 3"]);
}

#[test]
fn interpolation_and_variables() {
    let p = load("let name = \"World\"\nlet n = 2 * 3\np \"Hello {name} {n} {{x}}\"");
    assert_eq!(shape(&p), vec!["p Hello World 6 {x}"]);
}

#[test]
fn templates_extend_style_and_carry_before_after() {
    let src = "let body = box.bcolor:red.pad:1 .before:box { p \"top\" } .after:box { p \"bottom\" } { p \"never shown\" }\n\
               box.extends:body.pad:3 { p \"mine\" }";
    let p = load(src);
    assert_eq!(
        shape(&p),
        vec![
            "box",
            "  box",
            "    p top",
            "  p mine",
            "  box",
            "    p bottom"
        ]
    );
    let b = &p.root().children()[0];
    assert_eq!(b.style_get("bcolor"), Some(vec!["red".to_string()]));
    // The element's own style wins over the template's.
    assert_eq!(b.style_get("pad"), Some(vec!["3".to_string()]));
    assert!(b.style_get("extends").is_none());
}

#[test]
fn before_and_after_order_with_chained_templates() {
    let src = "let a = box .before:p \"a-before\" .after:p \"a-after\"\n\
               let b = box.extends:a .before:p \"b-before\" .after:p \"b-after\"\n\
               box.extends:b .before:p \"c-before\" .after:p \"c-after\" { p \"body\" }";
    let p = load(src);
    assert_eq!(
        shape(&p),
        vec![
            "box",
            "  p a-before",
            "  p b-before",
            "  p c-before",
            "  p body",
            "  p c-after",
            "  p b-after",
            "  p a-after"
        ]
    );
}

#[test]
fn unknown_and_wrong_templates_are_errors() {
    assert!(fail("box.extends:nope { }").contains("unknown template `nope`"));
    assert!(fail("let x = 5\nbox.extends:x { }").contains("not a template"));
}

#[test]
fn template_cycles_cannot_loop_forever() {
    // Defining is lazy; building the template follows `extends` into itself.
    assert!(fail("let a = box.extends:a {}\nbox.extends:a {}").contains("nests too deeply"));
}

#[test]
fn functions_return_elements_and_can_be_hoisted() {
    let src = "p \"x\"\ncard(\"hi\")\nfn card(t) { box { p t } }";
    // `card(..)` is an expression statement that is not an element: its value is dropped.
    let p = load(src);
    assert_eq!(shape(&p), vec!["p x"]);
    let p = load("fn card(t) { box { p t } }\nroot.tree.append(card(\"hi\")).unwrap()");
    assert_eq!(shape(&p), vec!["box", "  p hi"]);
}

#[test]
fn control_flow_appends_elements_in_bodies() {
    let src = "let mut i = 0\nwhile i < 3 { p \"row {i}\"\n i = i + 1 }\n\
               if i == 3 { p \"three\" } else { p \"other\" }\n\
               box { if false { p \"no\" } else if true { p \"yes\" } }";
    let p = load(src);
    assert_eq!(
        shape(&p),
        vec!["p row 0", "p row 1", "p row 2", "p three", "box", "  p yes"]
    );
}

#[test]
fn match_and_options() {
    let src = "let r = root.tree.query(\"p\")\n\
               match r { Ok(n) => p \"found\", Err(NotFound) => p \"missing\", Err(_) => p \"other\" }\n\
               match Some(2) { Some(1) => p \"one\", Some(n) => p \"n={n}\", None => p \"none\" }";
    let p = load(src);
    assert_eq!(shape(&p), vec!["p missing", "p n=2"]);
}

#[test]
fn match_without_arm_is_an_error() {
    assert!(fail("match 3 { 1 => p \"a\" }").contains("no `match` arm"));
}

#[test]
fn arithmetic_comparison_and_strings() {
    let src = "let a = 7 / 2\nlet b = 7 % 4\nlet c = 7.0 / 2\nlet d = \"ab\".upper() + \"cd\".len()\n\
               let e = 1 < 2 && \"a\" < \"b\" && !(2 == 3)\nlet f = \"a,b,c\".split(\",\").join(\"-\")\n\
               p \"{a} {b} {c} {d} {e} {f}\"";
    let p = load(src);
    assert_eq!(shape(&p), vec!["p 3 3 3.5 AB2 true a-b-c"]);
}

#[test]
fn arithmetic_errors() {
    assert!(fail("let x = 1 / 0").contains("division by zero"));
    assert!(fail("let x = 9223372036854775807 + 1").contains("overflow"));
    assert!(fail("let x = true + 1").contains("cannot add"));
    assert!(fail("let x = 1 < \"a\"").contains("cannot compare"));
    assert!(fail("if 1 { p \"x\" }").contains("Bool condition"));
    assert!(fail("let x = y").contains("unknown variable `y`"));
}

#[test]
fn immutability_is_enforced() {
    assert!(fail("let x = 1\nx = 2").contains("not `let mut`"));
    let p = load("let mut x = 1\nx = x + 1\np \"{x}\"");
    assert_eq!(shape(&p), vec!["p 2"]);
    assert!(fail("y = 2").contains("unknown variable"));
}

#[test]
fn closures_capture_their_scope() {
    let src = "fn make(n) { |x| x + n }\nlet add2 = make(2)\np \"{add2(5)}\"";
    assert_eq!(shape(&load(src)), vec!["p 7"]);
}

#[test]
fn recursion_works_and_runaway_recursion_stops() {
    let p = load("fn fact(n) { if n <= 1 { 1 } else { n * fact(n - 1) } }\np \"{fact(10)}\"");
    assert_eq!(shape(&p), vec!["p 3628800"]);
    let e = fail("fn f(n) { f(n + 1) }\nf(0)");
    assert!(e.contains("too deeply"), "{}", e);
}

/// Even a 1 MiB stack (the Windows main thread) must survive runaway recursion.
#[test]
fn runaway_recursion_fits_in_a_small_stack() {
    let h = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(|| {
            let e1 = fail("fn f(n) { f(n + 1) }\nf(0)");
            let e2 = fail("fn g(n) { if true { box { g(n) } } }\ng(0)");
            let e3 = fail("fn h(n) { [h(n)] }\nh(0)");
            let e4 = fail("let f = |g| g(g)\nf(f)");
            (e1, e2, e3, e4)
        })
        .unwrap();
    let (e1, e2, e3, e4) = h.join().expect("no stack overflow");
    for e in [e1, e2, e3, e4] {
        assert!(e.contains("too deeply"), "{}", e);
    }
}

#[test]
fn infinite_loops_are_stopped() {
    let e = fail("while true { }");
    assert!(e.contains("ran too long"), "{}", e);
}

#[test]
fn return_ends_a_function_early() {
    let src = "fn f(n) { if n > 1 { return \"big\" } \"small\" }\np \"{f(5)} {f(0)}\"";
    assert_eq!(shape(&load(src)), vec!["p big small"]);
}

#[test]
fn errors_name_the_line() {
    let e = fail("p \"a\"\np \"b\"\nlet x = 1 / 0");
    assert!(e.starts_with("line 3:"), "{}", e);
    let e = fail("p \"a\"\nbox.extends:zz {}");
    assert!(e.starts_with("line 2:"), "{}", e);
}

#[test]
fn p_cannot_have_a_body_or_children() {
    assert!(fail("p \"x\" { p \"y\" }").contains("cannot have a body"));
    assert!(fail("p.before:span \"a\" \"x\"").contains("cannot have children"));
}

#[test]
fn tree_query_create_append() {
    let src = "h1.id:title \"Old\"\n\
               let t = root.tree.query(\"#title\").unwrap()\n\
               t.set_text(\"New\")\n\
               let n = root.tree.create(Tag::p).unwrap()\n\
               n.set_text(\"made\")\n\
               root.tree.append(n)\n\
               let b = root.tree.create(Tag::box).unwrap()\n\
               b.append(root.tree.create(Tag::span).unwrap())\n\
               root.tree.append(b)";
    let p = load(src);
    assert_eq!(shape(&p), vec!["h1 New", "p made", "box", "  span"]);
    assert_eq!(text_of(&p, "h1#title"), "New");
}

#[test]
fn tree_errors_are_values() {
    let src = "let a = root.tree.create(Tag::p).unwrap()\n\
               let r1 = a.append(root.tree.create(Tag::p).unwrap())\n\
               let r2 = root.tree.query(\"#nothing\")\n\
               let r3 = root.tree.query(\"\")\n\
               let r4 = root.tree.create(\"blink\")\n\
               p \"{r1.is_err()} {r2.is_err()} {r3.is_err()} {r4.is_err()}\"\n\
               match r1 { Err(NotAContainer) => p \"nac\" _ => p \"?\" }\n\
               match r2 { Err(e) => p \"{e.kind}: {e.message}\" _ => p \"?\" }\n\
               match r4 { Err(UnknownTag) => p \"tag\" _ => p \"?\" }";
    let p = load(src);
    assert_eq!(
        shape(&p),
        vec![
            "p true true true true",
            "p nac",
            "p NotFound: nothing matches `#nothing`",
            "p tag"
        ]
    );
}

#[test]
fn append_moves_nodes_and_rejects_cycles() {
    let src = "box.id:a { box.id:b { p \"x\" } }\n\
               let a = root.tree.query(\"#a\").unwrap()\n\
               let b = root.tree.query(\"#b\").unwrap()\n\
               let c = b.append(a)\n\
               p \"{c.is_err()}\"\n\
               root.tree.append(b)";
    let p = load(src);
    assert_eq!(shape(&p), vec!["box", "p true", "box", "  p x"]);
}

#[test]
fn templates_can_be_built_and_appended() {
    let src = "let card = box.pad:1 { p \"hello\" }\n\
               root.tree.append(card.build())\n\
               root.tree.append(card)";
    // `card.build()` ignores nothing: the template's own body is built.
    let p = load(src);
    assert_eq!(shape(&p), vec!["box", "  p hello", "box", "  p hello"]);
}

#[test]
fn node_style_and_args_accessors() {
    let src = "p.fcolor:red \"x\"\n\
               let n = root.tree.query(\"p\").unwrap()\n\
               n.set_style(\"fcolor\", \"blue\")\n\
               n.set_style(\"size\", \"10, 2\")\n\
               n.set_arg(0, \"changed\")\n\
               let a = n.get_style(\"fcolor\")\nlet b = n.get_style(\"size\")\nlet c = n.get_style(\"nothing\")\n\
               p \"{a} {b} {c} {n.get_arg(0)} {n.tag()}\"";
    let p = load(src);
    assert_eq!(shape(&p)[1], "p blue 10,2 None changed p");
    assert_eq!(
        p.root().children()[0].style_get("size"),
        Some(vec!["10".to_string(), "2".to_string()])
    );
}

#[test]
fn set_text_on_each_kind() {
    let src = "link \"http://x.test\" \"label\"\ninput \"name\" \"v\"\nbox { }\n\
               root.tree.query(\"link\").unwrap().set_text(\"L\")\n\
               root.tree.query(\"input\").unwrap().set_text(\"V\")\n\
               let r = root.tree.query(\"box\").unwrap().set_text(\"B\")\n\
               p \"{r.is_err()}\"";
    let p = load(src);
    assert_eq!(
        shape(&p),
        vec!["link http://x.test|L", "input name|V", "box", "p true"]
    );
}

#[test]
fn log_collects_lines() {
    let mut p = load("log(\"a\", 1, true)\nlog(\"b\")");
    assert_eq!(p.take_log(), vec!["a 1 true", "b"]);
    assert!(p.take_log().is_empty());
}

// ── onclick ─────────────────────────────────────────────────────────

#[test]
fn onclick_calls_a_function_with_self_and_literals() {
    let src = "button.id:b.onclick:greet(\"Hi\", 3, true) \"Go\"\n\
               p.id:out \"-\"\n\
               fn greet(self, word, n, flag) {\n\
                 let out = root.tree.query(\"#out\").unwrap()\n\
                 out.set_text(\"{word} {n} {flag} {self.tag}\")\n\
               }";
    let mut p = load(src);
    let b = p.root().query("#b").unwrap();
    assert!(Page::is_clickable(&b));
    p.click(&b).unwrap();
    assert_eq!(text_of(&p, "#out"), "Hi 3 true button");
}

#[test]
fn onclick_without_arguments_and_float_arguments() {
    let src = "button.id:b.onclick:poke \"x\"\nbutton.id:c.onclick:half(1.5) \"y\"\np.id:out \"0\"\n\
               fn poke(self) { root.tree.query(\"#out\").unwrap().set_text(\"poked\") }\n\
               fn half(self, x) { root.tree.query(\"#out\").unwrap().set_text(\"{x * 2}\") }";
    let mut p = load(src);
    let b = p.root().query("#b").unwrap();
    p.click(&b).unwrap();
    assert_eq!(text_of(&p, "#out"), "poked");
    let c = p.root().query("#c").unwrap();
    p.click(&c).unwrap();
    assert_eq!(text_of(&p, "#out"), "3");
}

#[test]
fn onclick_errors_are_reported() {
    let mut p = load(
        "button.id:b.onclick:missing \"x\"\nbutton.id:c.onclick:two(1) \"y\"\nfn two(self) { 1 }",
    );
    let b = p.root().query("#b").unwrap();
    assert!(p.click(&b).unwrap_err().contains("no function `missing`"));
    let c = p.root().query("#c").unwrap();
    assert!(
        p.click(&c)
            .unwrap_err()
            .contains("takes 1 argument but 2 were given")
    );
    // A node without a handler is not an error.
    let mut p = load("p.id:a \"x\"");
    let a = p.root().query("#a").unwrap();
    p.click(&a).unwrap();
}

#[test]
fn onclick_can_loop_state_between_clicks() {
    let src = "let mut count = 0\nbutton.id:b.onclick:inc \"+\"\np.id:out \"0\"\n\
               fn inc(self) { count = count + 1\n root.tree.query(\"#out\").unwrap().set_text(\"{count}\") }";
    let mut p = load(src);
    let b = p.root().query("#b").unwrap();
    p.click(&b).unwrap();
    p.click(&b).unwrap();
    p.click(&b).unwrap();
    assert_eq!(text_of(&p, "#out"), "3");
}

// ── plugins ─────────────────────────────────────────────────────────

#[test]
fn oneshot_runs_source_and_defines_functions() {
    let src = "let r = root.plugin.oneshot(Plugin::source(\"p \\\"from plugin\\\"\\nfn hello(self) {{ 1 }}\"))\n\
               p \"{r.is_ok()}\"";
    let p = load(src);
    assert_eq!(shape(&p), vec!["p from plugin", "p true"]);
}

#[test]
fn oneshot_reports_compile_and_runtime_errors() {
    let src = "let a = root.plugin.oneshot(Plugin::source(\"fn (\"))\n\
               let b = root.plugin.oneshot(Plugin::source(\"let x = 1 / 0\"))\n\
               let c = root.plugin.oneshot(Plugin::url(\"http://other.test/none.cre\"))\n\
               match a { Err(Compile) => p \"a:compile\" _ => p \"a?\" }\n\
               match b { Err(Runtime) => p \"b:runtime\" _ => p \"b?\" }\n\
               match c { Err(Fetch) => p \"c:fetch\" _ => p \"c?\" }";
    assert_eq!(
        shape(&load(src)),
        vec!["p a:compile", "p b:runtime", "p c:fetch"]
    );
}

#[test]
fn oneshot_of_compiled_and_url_plugins() {
    let e = env();
    let Plugin::Compiled(bytes) = Plugin::compile("p \"binary\"").unwrap() else {
        panic!()
    };
    e.loader.set("http://site.test/c.crb", &bytes);
    e.loader.set("http://site.test/s.cre", b"p \"text\"");
    let src = "root.plugin.oneshot(Plugin::url(\"/c.crb\")).unwrap()\nroot.plugin.oneshot(Plugin::url(\"s.cre\")).unwrap()\n\
               let c = Plugin::compile(\"p \\\"inline\\\"\").unwrap()\nroot.plugin.oneshot(c).unwrap()\n\
               p \"{c.kind}\"";
    let p = load_at(&e, "http://site.test/", src).unwrap();
    assert_eq!(
        shape(&p),
        vec!["p binary", "p text", "p inline", "p compiled"]
    );
}

#[test]
fn installed_plugins_run_on_matching_pages_in_order() {
    let e = env();
    let src = "root.plugin.install(\"shop\", Plugin::source(\"p \\\"one\\\"\")).unwrap()\n\
               root.plugin.install(\"shop\", Plugin::source(\"p \\\"two\\\"\")).unwrap()\n\
               root.plugin.install(\"\", Plugin::source(\"p \\\"site-wide\\\"\")).unwrap()\n\
               p \"installed\"";
    // Installing does not run anything now.
    let p = load_at(&e, "http://site.test/shop", src).unwrap();
    assert_eq!(shape(&p), vec!["p installed"]);
    // A reload runs them, after the document, in insertion order.
    let p = load_at(&e, "http://site.test/shop/cart", "p \"page\"").unwrap();
    assert_eq!(shape(&p), vec!["p page", "p one", "p two", "p site-wide"]);
    // Outside the application path only the site-wide plugin runs.
    let p = load_at(&e, "http://site.test/blog", "p \"page\"").unwrap();
    assert_eq!(shape(&p), vec!["p page", "p site-wide"]);
    // Another site is untouched.
    let p = load_at(&e, "http://other.test/shop", "p \"page\"").unwrap();
    assert_eq!(shape(&p), vec!["p page"]);
}

#[test]
fn plugin_api_list_update_delete_clear() {
    let e = env();
    let src = "let a = root.plugin.install(\"app\", Plugin::source(\"p \\\"a\\\"\")).unwrap()\n\
               let b = root.plugin.install(\"app\", Plugin::source(\"p \\\"b\\\"\")).unwrap()\n\
               let l = root.plugin.list(\"app\").unwrap()\n\
               log(l.len(), l.first().unwrap().kind)\n\
               root.plugin.update(a, Plugin::source(\"p \\\"a2\\\"\")).unwrap()\n\
               root.plugin.delete(b).unwrap()\n\
               log(root.plugin.list(\"app\").unwrap().len())\n\
               let gone = root.plugin.delete(b)\n\
               log(gone.is_err())\n\
               match gone { Err(NotFound) => log(\"notfound\") _ => log(\"?\") }\n\
               root.plugin.clear(\"app\").unwrap()\n\
               log(root.plugin.list(\"app\").unwrap().len())\n\
               root.plugin.clear(\"app\").unwrap()";
    let mut p = load_at(&e, "http://site.test/", src).unwrap();
    assert_eq!(p.take_log(), vec!["2 source", "1", "true", "notfound", "0"]);
}

#[test]
fn update_changes_what_runs() {
    let e = env();
    load_at(
        &e,
        "http://site.test/",
        "let id = root.plugin.install(\"\", Plugin::source(\"p \\\"v1\\\"\")).unwrap()\n\
         root.plugin.update(id, Plugin::source(\"p \\\"v2\\\"\")).unwrap()",
    )
    .unwrap();
    let p = load_at(&e, "http://site.test/x", "").unwrap();
    assert_eq!(shape(&p), vec!["p v2"]);
}

#[test]
fn pages_cannot_touch_other_authorities() {
    let e = env();
    let mut p = load_at(
        &e,
        "http://a.test/",
        "let id = root.plugin.install(\"\", Plugin::source(\"p \\\"a\\\"\")).unwrap()\nlog(id)",
    )
    .unwrap();
    let id = p.take_log().remove(0);
    let src = format!(
        "let r = root.plugin.delete(\"{}\")\nlog(r.is_err())\nlog(root.plugin.list(\"\").unwrap().len())\n\
         root.plugin.clear(\"\").unwrap()",
        id
    );
    let mut q = load_at(&e, "http://b.test/", &src).unwrap();
    assert_eq!(q.take_log(), vec!["true", "0"]);
    // The plugin of a.test is still there.
    let p = load_at(&e, "http://a.test/z", "").unwrap();
    assert_eq!(shape(&p), vec!["p a"]);
}

#[test]
fn installed_plugin_functions_serve_buttons() {
    let e = env();
    load_at(
        &e,
        "http://site.test/",
        "root.plugin.install(\"\", Plugin::source(\"fn shout(self, w) {{ self.set_text(w.upper()) }}\")).unwrap()",
    )
    .unwrap();
    let mut p = load_at(
        &e,
        "http://site.test/",
        "button.id:b.onclick:shout(\"hey\") \"Go\"",
    )
    .unwrap();
    let b = p.root().query("#b").unwrap();
    p.click(&b).unwrap();
    assert_eq!(text_of(&p, "#b"), "HEY");
}

#[test]
fn plugin_failures_are_diagnostics_not_page_errors() {
    let e = env();
    {
        let mut r = e.registry.borrow_mut();
        let base = Url::parse("http://site.test/").unwrap();
        let k = plugin::AppKey::new("site.test", "").unwrap();
        r.install(
            k.clone(),
            Plugin::Source("let x = 1 / 0".into()),
            &base,
            &*e.loader,
        )
        .unwrap();
        r.install(
            k.clone(),
            Plugin::Source("p \"ok\"".into()),
            &base,
            &*e.loader,
        )
        .unwrap();
        r.install(k, Plugin::Url("/missing.cre".into()), &base, &*e.loader)
            .unwrap();
    }
    let p = load_at(&e, "http://site.test/", "p \"page\"").unwrap();
    assert_eq!(shape(&p), vec!["p page", "p ok"]);
    assert_eq!(p.diagnostics.len(), 2, "{:?}", p.diagnostics);
    assert!(p.diagnostics[0].contains("division by zero"));
    assert!(p.diagnostics[1].contains("cannot fetch"));
}

#[test]
fn plugins_run_offline_from_saved_files() {
    let dir = std::env::temp_dir().join(format!("brewster-script-test-{}", plugin::Uuid::new_v4()));
    let loader = Rc::new(MapLoader::default());
    loader.set("http://site.test/p.cre", b"p \"cached\"");
    {
        let reg = Rc::new(RefCell::new(Registry::open(&dir).unwrap()));
        Page::load(
            "root.plugin.install(\"\", Plugin::url(\"/p.cre\")).unwrap()\n\
             root.plugin.install(\"\", Plugin::source(\"p \\\"local\\\"\")).unwrap()",
            Url::parse("http://site.test/").unwrap(),
            reg,
            loader.clone(),
        )
        .unwrap();
    }
    // New session, no network.
    let offline = Rc::new(MapLoader::default());
    let reg = Rc::new(RefCell::new(Registry::open(&dir).unwrap()));
    let p = Page::load(
        "p \"page\"",
        Url::parse("http://site.test/").unwrap(),
        reg,
        offline,
    )
    .unwrap();
    assert_eq!(shape(&p), vec!["p page", "p cached", "p local"]);
    assert!(p.diagnostics.is_empty(), "{:?}", p.diagnostics);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_plugin_can_install_plugins_while_running() {
    let e = env();
    let src = "root.plugin.oneshot(Plugin::source(\"root.plugin.install(\\\"\\\", Plugin::source(\\\"p \\\\\\\"deep\\\\\\\"\\\")).unwrap()\")).unwrap()";
    load_at(&e, "http://site.test/", src).unwrap();
    let p = load_at(&e, "http://site.test/", "").unwrap();
    assert_eq!(shape(&p), vec!["p deep"]);
}

#[test]
fn bad_plugin_arguments_are_errors() {
    assert!(fail("root.plugin.oneshot(5)").contains("expects a Plugin"));
    assert!(fail("root.plugin.install(\"a\")").contains("takes 2 arguments"));
    assert!(fail("root.plugin.nothing()").contains("no function"));
    assert!(fail("root.plugin.delete(\"nope\")").contains("not a Uuid"));
    let src = "let r = root.plugin.install(\"../x\", Plugin::source(\"\"))\nlog(r.is_err())";
    let mut p = load(src);
    assert_eq!(p.take_log(), vec!["true"]);
}

#[test]
fn builtins_and_conversions() {
    let src = "log(int(\"42\").unwrap() + 1, int(\"x\").is_err(), float(\"1.5\").unwrap(), str(3) + \"!\", len(\"héé\"), len([1,2]))\n\
               log([1, 2, 3].contains(2), [1, 2].get(5).is_none(), [\"a\"].first().unwrap(), Some(3).unwrap_or(0), None.unwrap_or(9))\n\
               log(\"abc\".repeat(2), \"a-b\".replace(\"-\", \"+\"), \"  x \".trim(), \"Hello\".starts_with(\"He\"))";
    let mut p = load(src);
    assert_eq!(
        p.take_log(),
        vec!["43 true 1.5 3! 3 2", "true true a 3 9", "abcabc a+b x true"]
    );
}

#[test]
fn unwrap_failures_and_unknowns_are_errors() {
    assert!(fail("None.unwrap()").contains("unwrap"));
    assert!(fail("Err(\"boom\").unwrap()").contains("boom"));
    assert!(fail("nothing(1)").contains("unknown function"));
    assert!(fail("let x = 5\nx.foo()").contains("has no method"));
    assert!(fail("let x = root.nothing").contains("no field"));
    assert!(fail("let x = Tag::nothing").contains("not an element"));
    assert!(fail("let x = Tag::p()").contains("unknown function"));
}

#[test]
fn page_source_errors_are_reported() {
    assert!(fail("box {").contains("missing `}`"));
    assert!(fail("p \"x").contains("line 1"));
}

#[test]
fn elements_snapshot_matches_the_tree() {
    let p = load("box.pad:1 { p \"a\" }");
    let els = p.elements();
    assert_eq!(els.len(), 1);
    assert_eq!(els[0].tag, "box");
    assert_eq!(els[0].children[0].args, vec!["a".to_string()]);
    assert_eq!(els[0].id, p.root().children()[0].id());
}

#[test]
fn values_equal_compares_structurally() {
    assert!(values_equal(&Value::Int(1), &Value::Float(1.0)));
    assert!(!values_equal(&Value::Int(1), &Value::Str("1".into())));
    assert!(values_equal(
        &Value::List(vec![Value::Int(1)]),
        &Value::List(vec![Value::Int(1)])
    ));
}
