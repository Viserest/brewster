use super::*;

use cre::*;

fn lay(doc: &Document, width: usize, form: &Form) -> Page {
    doc.layout(width, form, LayoutOptions::default()).unwrap()
}

fn doc(build: fn() -> Element) -> Document {
    Document::from_page(Url::about("blank"), build)
}

#[test]
fn paragraph_fills_width() {
    let rows = render_page(|| p!("hi"), 6).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(row_text(&rows[0]), "hi    ");
}

#[test]
fn extends_skips_template_children() {
    let rows = render_page(
        || {
            let t = bx!({ pad: "1" }, [p!("skipped")]);
            bx!({ extends: &t }, [p!("kept")])
        },
        10,
    )
    .unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(row_text(&rows[1]), format!(" {} ", "kept    "));
    assert!(rows.iter().all(|r| !row_text(r).contains("skipped")));
}

#[test]
fn before_and_after_come_from_template() {
    let rows = render_page(
        || {
            let t = bx!({ after: bx!([p!("end")]) });
            bx!({ extends: &t }, [p!("mid")])
        },
        5,
    )
    .unwrap();
    let text: Vec<String> = rows.iter().map(row_text).collect();
    assert_eq!(text, vec!["mid  ".to_string(), "end  ".to_string()]);
}

#[test]
fn bad_attribute_values_are_reported_when_the_page_is_laid_out() {
    let e = render_page(|| p!({ fcolor: "no-such-color" }, "x"), 10).unwrap_err();
    assert!(e.contains("unknown color `no-such-color`"), "{e}");
    let e = render_page(|| bx!({ border: "2" }, [p!("x")]), 10).unwrap_err();
    assert!(e.contains("border"), "{e}");
    let e = render_page(|| bx!({ pad: "1,2,3" }, [p!("x")]), 10).unwrap_err();
    assert!(e.contains("pad"), "{e}");
}

#[test]
fn document_relayouts_at_any_width() {
    let doc = doc(|| p!("aaa bbb"));
    let wide = lay(&doc, 20, &Form::default());
    let narrow = lay(&doc, 4, &Form::default());
    assert_eq!(wide.rows.len(), 1);
    assert_eq!(narrow.rows.len(), 2);
}

#[test]
fn focus_and_input_ids_run_across_top_level_elements() {
    let page = lay(
        &doc(|| {
            bx!([
                link!("https://a.b", "A"),
                input!(Text, "n", "v"),
                button!("B"),
                input!(Text, "m", "w"),
            ])
        }),
        20,
        &Form::default(),
    );
    let ids: Vec<(Option<usize>, Option<usize>)> = page
        .rows
        .iter()
        .map(|r| {
            let i = r.info.as_ref().unwrap();
            (i.focus, i.input)
        })
        .collect();
    assert_eq!(
        ids,
        vec![(Some(0), None), (Some(1), Some(0)), (Some(2), None), (Some(3), Some(1))]
    );
    assert_eq!(page.inputs.len(), 2);
}

#[test]
fn form_values_reach_the_layout() {
    let doc = doc(|| input!(Text, "name", "John"));
    let mut form = Form::default();
    form.values.insert(0, "Jane".to_string());
    let page = lay(&doc, 30, &form);
    assert_eq!(row_text(&page.rows[0]).trim_end(), "name: [Jane]");
}

#[test]
fn password_inputs_are_protected() {
    let page = lay(&doc(|| input!(Password, "pw", "hunter2")), 30, &Form::default());
    assert!(page.inputs[0].protected);
}

#[test]
fn headers_and_sizes_work_end_to_end() {
    let rows = render_page(
        || bx!([h1!("Hi"), p!({ size: "6,2", bcolor: "red" }, "a")]),
        10,
    )
    .unwrap();
    let text: Vec<String> = rows.iter().map(row_text).collect();
    assert_eq!(text[0], "HI        ");
    assert_eq!(text[1], "a         ");
    assert_eq!(text[2], "          ");
}

#[test]
fn links_carry_their_target_and_node() {
    let doc = doc(|| link!("https://a.b/c?x=1&amp;y=2", "A"));
    let page = lay(&doc, 20, &Form::default());
    let info = page.rows[0].info.as_ref().unwrap();
    assert_eq!(info.href.as_deref(), Some("https://a.b/c?x=1&y=2"));
}

#[test]
fn buttons_run_handlers_and_change_the_tree() {
    let mut doc = doc(|| {
        button!({ onclick: |this: &Element, _: &Root| this.set_text("clicked 5") }, "idle")
    });
    let page = lay(&doc, 20, &Form::default());
    let info = page.rows[0].info.as_ref().unwrap();
    assert_eq!(info.text, "idle");
    assert!(doc.is_clickable(info.node));
    assert_eq!(doc.click(info.node), Ok(true));
    let page = lay(&doc, 20, &Form::default());
    assert_eq!(page.rows[0].info.as_ref().unwrap().text, "clicked 5");
}

#[test]
fn clicking_something_without_a_handler_does_nothing() {
    let mut doc = doc(|| p!("x"));
    let page = lay(&doc, 20, &Form::default());
    let node = page.rows[0].info.as_ref().unwrap().node;
    assert_eq!(doc.click(node), Ok(false));
    assert_eq!(doc.click(999_999), Ok(false));
}

#[test]
fn title_is_the_first_heading() {
    assert_eq!(doc(|| p!("x")).title(), None);
    assert_eq!(doc(|| bx!([h3!("low"), h2!("mid")])).title().as_deref(), Some("mid"));
    assert_eq!(
        doc(|| bx!([bx!([h1!("Top")]), h1!("Other")])).title().as_deref(),
        Some("Top")
    );
}

// ── loading ─────────────────────────────────────────────────────────

use std::collections::HashMap;

struct Files(HashMap<String, Vec<u8>>);

impl Loader for Files {
    fn load(&self, url: &Url) -> Result<String, String> {
        self.load_bytes(url).map(|b| String::from_utf8_lossy(&b).into_owned())
    }
    fn load_bytes(&self, url: &Url) -> Result<Vec<u8>, String> {
        self.0
            .get(&url.to_string())
            .cloned()
            .ok_or_else(|| format!("{} not found", url))
    }
}

fn host() -> Rc<Host> {
    let runner = MapRunner::new()
        .page("page", || bx!([h1!("From a module"), p!("body")]))
        .page("banner", || bx!([p!({ fcolor: "yellow" }, "plugin text")]));
    Rc::new(Host::new(
        plugin::Compiler::new("no-such-rustc", None, std::env::temp_dir().join("brewster-engine-test")),
        Box::new(runner),
    ))
}

fn files() -> Rc<Files> {
    let mut m = HashMap::new();
    m.insert("file:///page.wasm".to_string(), MapRunner::module("page"));
    m.insert("file:///page.rs".to_string(), b"fn page() {}".to_vec());
    m.insert("file:///banner.wasm".to_string(), MapRunner::module("banner"));
    m.insert("file:///junk.wasm".to_string(), MapRunner::module("junk"));
    Rc::new(Files(m))
}

#[test]
fn a_module_is_fetched_started_and_titled() {
    let rows = render_url(&Url::parse("file:///page.wasm").unwrap(), 20, files(), host()).unwrap();
    let text: Vec<String> = rows.iter().map(row_text).collect();
    assert_eq!(text[0], "FROM A MODULE       ");
    assert!(text.iter().any(|t| t.starts_with("body")));
}

#[test]
fn source_that_cannot_be_compiled_uses_the_precompiled_module_beside_it() {
    // No rustc here, so page.rs falls back to page.wasm.
    let rows = render_url(&Url::parse("file:///page.rs").unwrap(), 20, files(), host()).unwrap();
    assert!(rows.iter().any(|r| row_text(r).starts_with("body")));
}

#[test]
fn a_page_that_cannot_run_is_an_error() {
    let e = render_url(&Url::parse("file:///junk.wasm").unwrap(), 20, files(), host()).unwrap_err();
    assert!(e.contains("unknown module"), "{e}");
    let e = render_url(&Url::parse("file:///missing.wasm").unwrap(), 20, files(), host()).unwrap_err();
    assert!(e.contains("not found"), "{e}");
}

#[test]
fn installed_plugins_run_after_the_page_and_failures_are_diagnostics() {
    let url = Url::parse("file:///page.wasm").unwrap();
    let loader = files();
    let reg = Rc::new(RefCell::new(Registry::in_memory()));
    let app = plugin::AppKey::new("", "").unwrap();
    {
        let mut r = reg.borrow_mut();
        r.install(app.clone(), plugin::Plugin::Url("banner.wasm".into()), &url, &*loader).unwrap();
        // A module the runner does not know, and one that is not wasm at all.
        r.install(app.clone(), plugin::Plugin::Url("junk.wasm".into()), &url, &*loader).unwrap();
        r.install(app, plugin::Plugin::Source("fn page() {}".into()), &url, &*loader).unwrap();
    }
    let doc = Document::load(url, reg, loader, host()).unwrap();
    let page = lay(&doc, 30, &Form::default());
    let text: Vec<String> = page.rows.iter().map(row_text).collect();
    let body = text.iter().position(|t| t.starts_with("body")).unwrap();
    let plug = text.iter().position(|t| t.starts_with("plugin text")).unwrap();
    assert!(plug > body, "{:?}", text);
    assert_eq!(doc.diagnostics().len(), 2, "{:?}", doc.diagnostics());
    assert!(doc.diagnostics().iter().all(|d| d.starts_with("plugin ")));
}
