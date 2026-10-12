use super::*;

fn lay(doc: &Document, width: usize, form: &Form) -> Page {
    doc.layout(width, form, LayoutOptions::default()).unwrap()
}

#[test]
fn paragraph_fills_width() {
    let rows = render_source("p \"hi\"", 6).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(row_text(&rows[0]), "hi    ");
}

#[test]
fn extends_skips_template_children() {
    let src = "let t = box.pad:1 { p \"skipped\" }\nbox.extends:t { p \"kept\" }";
    let rows = render_source(src, 10).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(row_text(&rows[1]), format!(" {} ", "kept    "));
    assert!(rows.iter().all(|r| !row_text(r).contains("skipped")));
}

#[test]
fn before_and_after_come_from_template() {
    let src = "let t = box.after:box { p \"end\" }\nbox.extends:t { p \"mid\" }";
    let rows = render_source(src, 5).unwrap();
    let text: Vec<String> = rows.iter().map(row_text).collect();
    assert_eq!(text, vec!["mid  ".to_string(), "end  ".to_string()]);
}

#[test]
fn unknown_template_is_an_error() {
    assert!(render_source("box.extends:nope {}", 10).is_err());
}

#[test]
fn document_relayouts_at_any_width() {
    let doc = Document::parse("p \"aaa bbb\"").unwrap();
    let wide = lay(&doc, 20, &Form::default());
    let narrow = lay(&doc, 4, &Form::default());
    assert_eq!(wide.rows.len(), 1);
    assert_eq!(narrow.rows.len(), 2);
}

#[test]
fn focus_and_input_ids_run_across_top_level_elements() {
    let src = "link https://a.b \"A\"\ninput \"n\" \"v\"\nbutton \"B\"\ninput \"m\" \"w\"";
    let page = lay(&Document::parse(src).unwrap(), 20, &Form::default());
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
    let doc = Document::parse("input \"name\" \"John\"").unwrap();
    let mut form = Form::default();
    form.values.insert(0, "Jane".to_string());
    let page = lay(&doc, 30, &form);
    assert_eq!(row_text(&page.rows[0]).trim_end(), "name: [Jane]");
}

#[test]
fn headers_and_sizes_work_end_to_end() {
    let rows = render_source("h1 \"Hi\"\np.size:6,2.bcolor:red \"a\"", 10).unwrap();
    let text: Vec<String> = rows.iter().map(row_text).collect();
    assert_eq!(text[0], "HI        ");
    assert_eq!(text[1], "a         ");
    assert_eq!(text[2], "          ");
}

#[test]
fn links_carry_their_target_and_node() {
    let doc = Document::parse("link https://a.b/c?x=1&amp;y=2 \"A\"").unwrap();
    let page = lay(&doc, 20, &Form::default());
    let info = page.rows[0].info.as_ref().unwrap();
    assert_eq!(info.href.as_deref(), Some("https://a.b/c?x=1&y=2"));
}

#[test]
fn buttons_run_plugin_functions_and_change_the_tree() {
    let src = "fn bump(self: Element, by) { self.set_text(\"clicked {by}\") }\nbutton.onclick:bump(5) \"idle\"";
    let mut doc = Document::parse(src).unwrap();
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
    let mut doc = Document::parse("p \"x\"").unwrap();
    let page = lay(&doc, 20, &Form::default());
    let node = page.rows[0].info.as_ref().unwrap().node;
    assert_eq!(doc.click(node), Ok(false));
    assert_eq!(doc.click(999_999), Ok(false));
}

#[test]
fn a_failing_handler_is_an_error_not_a_crash() {
    let mut doc = Document::parse("button.onclick:missing \"B\"").unwrap();
    let page = lay(&doc, 20, &Form::default());
    let node = page.rows[0].info.as_ref().unwrap().node;
    assert!(doc.click(node).unwrap_err().contains("missing"));
}

#[test]
fn title_is_the_first_heading() {
    assert_eq!(Document::parse("p \"x\"").unwrap().title(), None);
    assert_eq!(
        Document::parse("h3 \"low\"\nh2 \"mid\"").unwrap().title().as_deref(),
        Some("mid")
    );
    assert_eq!(
        Document::parse("box { h1 \"Top\" }\nh1 \"Other\"").unwrap().title().as_deref(),
        Some("Top")
    );
}
