use super::*;

use std::sync::atomic::{AtomicUsize, Ordering};

use tree::Style;

static NEXT_ID: AtomicUsize = AtomicUsize::new(1000);

fn el(tag: &str, style: &[(&str, &[&str])], args: &[&str], children: Vec<Element>) -> Element {
    let mut s = Style::new();
    for (k, v) in style {
        s.insert(k.to_string(), v.iter().map(|x| x.to_string()).collect());
    }
    Element {
        tag: tag.to_string(),
        style: s,
        args: args.iter().map(|x| x.to_string()).collect(),
        children,
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
    }
}

fn texts(rows: &[Row]) -> Vec<String> {
    rows.iter().map(row_text).collect()
}

fn lay(roots: &[Element], width: usize) -> Page {
    layout_all(roots, width, &Form::default()).unwrap()
}

const RED: Rgb = Rgb(220, 50, 47);
const BLUE: Rgb = Rgb(60, 120, 240);

// ── 1. styles end right after the text ──────────────────────────────

#[test]
fn background_ends_right_after_text() {
    let p = el("p", &[("bcolor", &["red"])], &["hi"], vec![]);
    let rows = lay(&[p], 10).rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(row_text(&rows[0]), "hi        ");
    let colored: Vec<&Span> = rows[0].spans.iter().filter(|s| s.bg == Some(RED)).collect();
    assert_eq!(colored.len(), 1);
    assert_eq!(colored[0].text, "hi");
}

#[test]
fn underline_covers_only_the_text() {
    let l = el("link", &[], &["https://x.y", "Home"], vec![]);
    let rows = lay(&[l], 12).rows;
    let underlined: String = rows[0]
        .spans
        .iter()
        .filter(|s| s.underline)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(underlined, "Home");
    assert_eq!(row_text(&rows[0]), "Home        ");
}

#[test]
fn border_and_pad_hug_the_content() {
    let p = el(
        "p",
        &[("border", &["1"]), ("pad-x", &["1"])],
        &["hi"],
        vec![],
    );
    let rows = lay(&[p], 12).rows;
    assert_eq!(
        texts(&rows),
        vec![
            "┌────┐      ".to_string(),
            "│ hi │      ".to_string(),
            "└────┘      ".to_string(),
        ]
    );
}

#[test]
fn box_is_as_wide_as_its_widest_child() {
    let b = el(
        "box",
        &[("bcolor", &["blue"])],
        &[],
        vec![
            el("p", &[], &["ab"], vec![]),
            el("p", &[], &["abcd"], vec![]),
        ],
    );
    let rows = lay(&[b], 10).rows;
    assert_eq!(
        texts(&rows),
        vec!["ab        ".to_string(), "abcd      ".to_string()]
    );
    // Blue covers exactly the 4-cell box (text + gap), not the whole row.
    let blue: usize = rows[0]
        .spans
        .iter()
        .filter(|s| s.bg == Some(BLUE))
        .map(|s| s.text.chars().count())
        .sum();
    assert_eq!(blue, 4);
}

#[test]
fn nested_margin_counts_toward_parent_width() {
    let b = el(
        "box",
        &[("border", &["1"])],
        &[],
        vec![el("p", &[("margin-x", &["2"])], &["x"], vec![])],
    );
    let rows = lay(&[b], 20).rows;
    assert_eq!(rows[0].width(), 20);
    assert_eq!(row_text(&rows[0]).trim_end(), "┌─────┐");
    assert_eq!(row_text(&rows[1]).trim_end(), "│  x  │");
}

// ── 2. size, with min and max ───────────────────────────────────────

#[test]
fn size_max_fills_the_parent() {
    let p = el(
        "p",
        &[("size", &["max"]), ("bcolor", &["red"])],
        &["hi"],
        vec![],
    );
    let rows = lay(&[p], 6).rows;
    assert_eq!(texts(&rows), vec!["hi    ".to_string()]);
    assert_eq!(rows[0].spans.len(), 2);
    assert!(rows[0].spans.iter().all(|s| s.bg == Some(RED)));
}

#[test]
fn size_max_inside_a_min_box_uses_the_box_width() {
    let b = el(
        "box",
        &[],
        &[],
        vec![
            el(
                "p",
                &[("size-x", &["max"]), ("bcolor", &["red"])],
                &["a"],
                vec![],
            ),
            el("p", &[], &["abcdef"], vec![]),
        ],
    );
    let rows = lay(&[b], 20).rows;
    let red: String = rows[0]
        .spans
        .iter()
        .filter(|s| s.bg == Some(RED))
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(red, "a     ");
}

#[test]
fn size_cells_sets_exact_width_and_height() {
    let p = el(
        "p",
        &[("size", &["6", "3"]), ("border", &["1"])],
        &["a"],
        vec![],
    );
    let rows = lay(&[p], 10).rows;
    assert_eq!(
        texts(&rows),
        vec![
            "┌────┐    ".to_string(),
            "│a   │    ".to_string(),
            "└────┘    ".to_string(),
        ]
    );
    // 6x3 box with a border leaves a 4x1 content area.
    let q = el("p", &[("size", &["4", "3"])], &["a"], vec![]);
    let rows = lay(&[q], 4).rows;
    assert_eq!(
        texts(&rows),
        vec!["a   ".to_string(), "    ".to_string(), "    ".to_string()]
    );
}

#[test]
fn fixed_height_truncates_extra_lines() {
    let p = el("p", &[("size", &["4", "1"])], &["a b c d e f"], vec![]);
    let rows = lay(&[p], 20).rows;
    assert_eq!(texts(&rows), vec!["a b ".to_string() + &" ".repeat(16)]);
}

#[test]
fn text_wraps_at_the_parent_edge_when_min() {
    let p = el("p", &[], &["aaa bbb ccc"], vec![]);
    let rows = lay(&[p], 7).rows;
    assert_eq!(
        texts(&rows),
        vec!["aaa bbb".to_string(), "ccc    ".to_string()]
    );
}

#[test]
fn size_never_exceeds_the_parent() {
    let p = el("p", &[("size", &["50"])], &["x"], vec![]);
    let rows = lay(&[p], 8).rows;
    assert_eq!(rows[0].width(), 8);
}

// ── 6. size drives input wrapping ───────────────────────────────────

#[test]
fn input_wraps_to_its_size() {
    let i = el(
        "input",
        &[("size-x", &["10"])],
        &["name", "hello world foo"],
        vec![],
    );
    let page = lay(&[i], 10);
    let lines: Vec<String> = page
        .rows
        .iter()
        .map(|r| row_text(r).trim_end().to_string())
        .collect();
    assert_eq!(lines, vec!["name:", "[hello", "world foo]"]);
    assert_eq!(page.inputs[0].segs, vec![(0, 6), (6, 13), (13, 23)]);
}

#[test]
fn input_without_size_stays_on_one_line() {
    let i = el("input", &[], &["name", "John Doe"], vec![]);
    let page = lay(&[i], 40);
    assert_eq!(page.rows.len(), 1);
    assert_eq!(row_text(&page.rows[0]).trim_end(), "name: [John Doe]");
}

#[test]
fn input_height_window_follows_the_caret() {
    let i = el(
        "input",
        &[("size", &["10", "2"])],
        &["name", "hello world foo"],
        vec![],
    );
    // Not editing: shows the top two lines.
    let page = lay(&[i.clone()], 10);
    assert_eq!(page.rows.len(), 2);
    assert_eq!(row_text(&page.rows[0]).trim_end(), "name:");
    // Caret at the end of the value: the window scrolls to the last two lines.
    let form = Form {
        values: HashMap::new(),
        editing: Some((0, 15)),
    };
    let page = layout_all(&[i], 10, &form).unwrap();
    assert_eq!(page.rows.len(), 2);
    assert_eq!(row_text(&page.rows[0]).trim_end(), "[hello");
    assert_eq!(row_text(&page.rows[1]).trim_end(), "world foo]");
}

#[test]
fn protected_input_hides_the_value_but_keeps_it_for_copy() {
    let i = el("input", &[("protected", &[])], &["pw", "hunter2"], vec![]);
    let page = lay(&[i], 30);
    assert_eq!(row_text(&page.rows[0]).trim_end(), "pw: [*******]");
    assert_eq!(page.rows[0].info.as_ref().unwrap().text, "hunter2");
}

#[test]
fn form_value_overrides_the_source_value() {
    let i = el("input", &[], &["name", "John"], vec![]);
    let mut form = Form::default();
    form.values.insert(0, "Jane".to_string());
    let page = layout_all(&[i], 30, &form).unwrap();
    assert_eq!(row_text(&page.rows[0]).trim_end(), "name: [Jane]");
    assert_eq!(page.inputs[0].value, "Jane");
}

// ── 7. headers ──────────────────────────────────────────────────────

fn header(tag: &str) -> Span {
    let h = el(tag, &[], &["Hi there"], vec![]);
    lay(&[h], 30).rows[0].spans[0].clone()
}

#[test]
fn headers_have_no_marks() {
    for t in ["h1", "h2", "h3", "h4", "h5", "h6"] {
        let h = el(t, &[], &["Hi there"], vec![]);
        let rows = lay(&[h], 30).rows;
        assert_eq!(rows.len(), 1);
        assert!(
            !row_text(&rows[0]).contains('='),
            "{}: {:?}",
            t,
            row_text(&rows[0])
        );
    }
}

#[test]
fn headers_differ_in_case_boldness_and_underline() {
    let table = [
        ("h1", "HI THERE", true, false),
        ("h2", "HI THERE", true, false),
        ("h3", "HI THERE", false, false),
        ("h4", "Hi there", true, false),
        ("h5", "Hi there", true, true),
        ("h6", "Hi there", false, true),
    ];
    for (tag, text, bold, underline) in table {
        let s = header(tag);
        assert_eq!(s.text, text, "{}", tag);
        assert_eq!((s.bold, s.underline), (bold, underline), "{}", tag);
        assert_eq!(s.scale, 1, "{} is not scaled without scale_headers", tag);
    }
    // No two headers look the same: (case, bold, underline) are pairwise different
    // except where the font size tells them apart (h1/h2 differ only in size).
    let looks: Vec<_> = table
        .iter()
        .map(|(_, t, b, u)| (!t.chars().any(char::is_lowercase), *b, *u))
        .collect();
    assert_eq!(
        looks.iter().filter(|l| **l == (true, true, false)).count(),
        2
    );
}

#[test]
fn header_copy_text_is_the_displayed_text() {
    let h = el("h1", &[], &["Hi"], vec![]);
    let rows = lay(&[h], 30).rows;
    assert_eq!(rows[0].info.as_ref().unwrap().text, "HI");
}

#[test]
fn header_wraps_at_the_full_width() {
    let h = el("h4", &[], &["aaa bbb ccc"], vec![]);
    let rows = lay(&[h], 7).rows;
    assert_eq!(
        texts(&rows),
        vec!["aaa bbb".to_string(), "ccc    ".to_string()]
    );
}

fn lay_scaled(roots: &[Element], width: usize) -> Page {
    layout_all_with(
        roots,
        width,
        &Form::default(),
        LayoutOptions {
            viewport_rows: None,
            scale_headers: true,
            ..LayoutOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn scaled_headers_take_scale_cells_per_char_and_scale_rows() {
    let h = el("h1", &[], &["ab"], vec![]);
    let rows = lay_scaled(&[h], 12).rows;
    // size 3: each char is 3 cells wide, the line is 3 rows tall.
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].spans[0].text, "AB");
    assert_eq!(rows[0].spans[0].scale, 3);
    assert_eq!(rows[0].spans[0].cells(), 6);
    assert!(rows.iter().all(|r| r.width() == 12));
    assert!(!rows[0].cont && rows[1].cont && rows[2].cont);
    assert!(rows[1].info.is_none() && rows[2].info.is_none());
    assert_eq!(rows[0].info.as_ref().unwrap().cols, (0, 6));
    assert_eq!(rows[0].info.as_ref().unwrap().text, "AB");
}

#[test]
fn scaled_headers_wrap_by_scaled_width() {
    let h = el("h2", &[], &["aa bb cc"], vec![]);
    // size 2 in 8 cells: 4 chars per line.
    let rows = lay_scaled(&[h], 8).rows;
    let lines: Vec<String> = rows
        .iter()
        .filter(|r| !r.cont)
        .map(|r| r.spans[0].text.clone())
        .collect();
    assert_eq!(lines, vec!["AA", "BB", "CC"]);
    assert_eq!(rows.len(), 6);
}

#[test]
fn scaling_never_exceeds_the_available_width() {
    let h = el("h1", &[], &["abc"], vec![]);
    let rows = lay_scaled(&[h], 2).rows;
    assert!(
        rows.iter().all(|r| r.width() == 2),
        "{:?}",
        rows.iter().map(|r| r.width()).collect::<Vec<_>>()
    );
}

#[test]
fn scaled_headers_align_and_pad_like_text() {
    let h = el(
        "h2",
        &[("align", &["middle"]), ("bcolor", &["red"])],
        &["ab"],
        vec![],
    );
    let rows = lay_scaled(&[h], 12).rows;
    // 4 cells wide in 12: 4 on each side.
    assert_eq!(rows[0].info.as_ref().unwrap().cols, (4, 8));
    assert!(rows.iter().all(|r| r.width() == 12));
}

// ── viewport: the document is as large as the screen ───────────────

fn lay_view(roots: &[Element], width: usize, rows: usize) -> Page {
    layout_all_with(
        roots,
        width,
        &Form::default(),
        LayoutOptions {
            viewport_rows: Some(rows),
            scale_headers: false,
            ..LayoutOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn top_level_max_height_fills_the_screen() {
    let b = el(
        "box",
        &[("size", &["max"]), ("bcolor", &["blue"])],
        &[],
        vec![el("p", &[], &["hi"], vec![])],
    );
    let page = lay_view(&[b], 10, 6);
    assert_eq!(page.rows.len(), 6);
    assert!(page.rows.iter().all(|r| r.width() == 10));
    assert!(
        page.rows
            .iter()
            .all(|r| r.spans.iter().all(|s| s.bg == Some(BLUE)))
    );
}

#[test]
fn max_height_fills_only_what_is_left_after_earlier_elements() {
    let b = el(
        "box",
        &[("size", &["max"]), ("bcolor", &["blue"])],
        &[],
        vec![],
    );
    let page = lay_view(&[el("p", &[], &["top"], vec![]), b], 10, 6);
    assert_eq!(page.rows.len(), 6);
    assert_eq!(row_text(&page.rows[0]).trim_end(), "top");
}

#[test]
fn max_height_never_cuts_long_content() {
    let kids: Vec<Element> = (0..10)
        .map(|i| el("p", &[], &[&format!("line {}", i)], vec![]))
        .collect();
    let b = el("box", &[("size", &["max"])], &[], kids);
    let page = lay_view(&[b], 10, 4);
    assert_eq!(page.rows.len(), 10);
}

#[test]
fn without_a_viewport_max_height_is_content_height() {
    let b = el(
        "box",
        &[("size", &["max"])],
        &[],
        vec![el("p", &[], &["hi"], vec![])],
    );
    assert_eq!(lay(&[b], 10).rows.len(), 1);
}

#[test]
fn min_height_elements_do_not_grow_with_the_viewport() {
    let page = lay_view(&[el("p", &[], &["hi"], vec![])], 10, 6);
    assert_eq!(page.rows.len(), 1);
}

#[test]
fn rows_carry_href_and_node_ids() {
    let l = el("link", &[], &["https://x.test/a&amp;b", "Go"], vec![]);
    let b = el("button", &[], &["Run"], vec![]);
    let (lid, bid) = (l.id, b.id);
    let page = lay(&[l, b], 20);
    let li = page.rows[0].info.as_ref().unwrap();
    assert_eq!(li.href.as_deref(), Some("https://x.test/a&b"));
    assert_eq!(li.node, lid);
    let bi = page.rows[1].info.as_ref().unwrap();
    assert_eq!(bi.href, None);
    assert_eq!(bi.node, bid);
}

#[test]
fn inputs_remember_their_node() {
    let i = el("input", &[], &["name", "v"], vec![]);
    let id = i.id;
    let page = lay(&[i], 20);
    assert_eq!(page.inputs[0].node, id);
    assert_eq!(page.rows[0].info.as_ref().unwrap().node, id);
}

// ── Row metadata used by the cursor ─────────────────────────────────

#[test]
fn rows_carry_kind_focus_and_copy_text() {
    let page = lay(
        &[
            el("p", &[], &["plain"], vec![]),
            el("link", &[], &["https://x.y", "Home"], vec![]),
            el("button", &[], &["Go"], vec![]),
            el("input", &[], &["n", "v"], vec![]),
            el("pre", &[], &["\nraw  text\n"], vec![]),
        ],
        20,
    );
    let infos: Vec<&LineInfo> = page.rows.iter().map(|r| r.info.as_ref().unwrap()).collect();
    assert_eq!(infos.len(), 5);
    assert_eq!(infos[0].kind, Kind::Text);
    assert_eq!(infos[0].focus, None);
    assert_eq!(
        (infos[1].kind, infos[1].focus, infos[1].text.as_str()),
        (Kind::Link, Some(0), "Home")
    );
    assert_eq!(
        (infos[2].kind, infos[2].focus, infos[2].text.as_str()),
        (Kind::Button, Some(1), "Go")
    );
    assert_eq!(
        (infos[3].kind, infos[3].focus, infos[3].input),
        (Kind::Input, Some(2), Some(0))
    );
    assert_eq!(infos[3].text, "v");
    assert_eq!(
        (infos[4].kind, infos[4].text.as_str()),
        (Kind::Pre, "raw  text")
    );
}

#[test]
fn spacing_rows_have_no_info() {
    let p = el(
        "p",
        &[("margin", &["1"]), ("pad", &["1"]), ("border", &["1"])],
        &["x"],
        vec![],
    );
    let rows = lay(&[p], 10).rows;
    let with_info = rows.iter().filter(|r| r.info.is_some()).count();
    assert_eq!(rows.len(), 7);
    assert_eq!(with_info, 1);
}

#[test]
fn cols_cover_exactly_the_text() {
    let p = el(
        "p",
        &[("align", &["middle"]), ("margin-x", &["1"])],
        &["ab"],
        vec![],
    );
    let rows = lay(&[p], 10).rows;
    let info = rows[0].info.as_ref().unwrap();
    let t = row_text(&rows[0]);
    let (a, b) = info.cols;
    assert_eq!(t.chars().skip(a).take(b - a).collect::<String>(), "ab");
}

#[test]
fn multi_row_link_shares_one_focus_id() {
    let l = el("link", &[], &["u", "aaa bbb ccc"], vec![]);
    let page = lay(&[l, el("button", &[], &["B"], vec![])], 7);
    let foci: Vec<Option<usize>> = page
        .rows
        .iter()
        .map(|r| r.info.as_ref().unwrap().focus)
        .collect();
    assert_eq!(foci, vec![Some(0), Some(0), Some(1)]);
    // Every row of the link copies the whole link text.
    assert_eq!(page.rows[1].info.as_ref().unwrap().text, "aaa bbb ccc");
}

#[test]
fn caret_column_accounts_for_the_prefix() {
    let i = el("input", &[], &["name", "abcdef"], vec![]);
    let form = Form {
        values: HashMap::new(),
        editing: Some((0, 3)),
    };
    let page = layout_all(&[i], 40, &form).unwrap();
    // `name: [` is 7 chars, so the caret after "abc" is at column 10.
    assert_eq!(page.rows[0].info.as_ref().unwrap().caret, Some(10));
}

#[test]
fn rows_are_exactly_as_wide_as_requested() {
    let page = lay(
        &[
            el("h1", &[("align", &["middle"])], &["Title"], vec![]),
            el(
                "box",
                &[("border", &["1"]), ("pad", &["1"])],
                &[],
                vec![el("p", &[], &["body text"], vec![])],
            ),
            el("input", &[("size", &["max"])], &["n", "v"], vec![]),
        ],
        33,
    );
    assert!(page.rows.iter().all(|r| r.width() == 33));
}


// ── graphical: shapes, controls, rounded corners ────────────────────

fn controls() -> Controls {
    let c = |fill: Rgb| ControlStyle {
        fg: Some(Rgb(1, 2, 3)),
        fill: Some(fill),
        stroke: None,
        radius: 8.0,
        fade: Fade::None,
    };
    Controls {
        button: c(Rgb(10, 10, 10)),
        link: c(Rgb(20, 20, 20)),
        input: ControlStyle { stroke: Some(Rgb(9, 9, 9)), ..c(Rgb(30, 30, 30)) },
        placeholder: Rgb(7, 7, 7),
    }
}

fn gfx(roots: &[Element], width: usize) -> Page {
    layout_all_with(
        roots,
        width,
        &Form::default(),
        LayoutOptions { graphical: Some(controls()), ..LayoutOptions::default() },
    )
    .unwrap()
}

#[test]
fn a_button_in_a_window_is_a_padded_shape_without_brackets() {
    let b = el("button", &[], &["Go"], vec![]);
    let page = gfx(&[b], 20);
    assert_eq!(texts(&page.rows), vec![" Go ".to_string() + &" ".repeat(16)]);
    assert_eq!(page.decos.len(), 1);
    let d = &page.decos[0];
    assert_eq!((d.col, d.row, d.cols, d.rows), (0, 0, 4, 1));
    assert_eq!(d.fill, Some(Rgb(10, 10, 10)));
    assert_eq!(d.kind, DecoKind::Button);
    assert_eq!(d.radius, 8.0);
    assert_eq!(d.focus, Some(0));
    // The text is the label, in the look's color, and clicks still find it.
    let span = page.rows[0].spans.iter().find(|s| s.text == "Go").unwrap();
    assert_eq!(span.fg, Some(Rgb(1, 2, 3)));
    assert_eq!(page.rows[0].info.as_ref().unwrap().cols, (1, 3));
}

#[test]
fn links_and_buttons_get_the_same_shape_with_their_own_fill() {
    let l = el("link", &[], &["/x", "home"], vec![]);
    let b = el("button", &[], &["home"], vec![]);
    let page = gfx(&[l, b], 20);
    assert_eq!(page.decos.len(), 2);
    assert_eq!((page.decos[0].cols, page.decos[0].rows), (page.decos[1].cols, page.decos[1].rows));
    assert_eq!(page.decos[0].kind, DecoKind::Link);
    assert_eq!(page.decos[1].row, 1);
    assert_ne!(page.decos[0].fill, page.decos[1].fill);
    // Links are not underlined in a window: the shape says it is one.
    assert!(page.rows[0].spans.iter().all(|s| !s.underline));
}

#[test]
fn an_input_shows_its_name_until_it_has_a_value() {
    let i = el("input", &[], &["email", "", "text"], vec![]);
    let page = gfx(&[i.clone()], 30);
    assert_eq!(page.inputs[0].prefix, 0);
    let row = &page.rows[0];
    assert!(texts(&page.rows)[0].contains("email"));
    let name = row.spans.iter().find(|s| s.text == "email").unwrap();
    assert_eq!(name.fg, Some(Rgb(7, 7, 7)));
    assert_eq!(page.decos[0].kind, DecoKind::Input);
    assert_eq!(page.decos[0].stroke, Some(Rgb(9, 9, 9)));
    assert_eq!(page.decos[0].cols, 18); // sixteen cells and a cell of padding each side

    let mut form = Form::default();
    form.values.insert(0, "a@b.c".to_string());
    let page = layout_all_with(
        &[i],
        30,
        &form,
        LayoutOptions { graphical: Some(controls()), ..LayoutOptions::default() },
    )
    .unwrap();
    assert!(texts(&page.rows)[0].contains("a@b.c"));
    assert!(!texts(&page.rows)[0].contains("email"));
}

#[test]
fn a_protected_input_shows_bullets_and_keeps_the_caret_cell() {
    let i = el("input", &[("protected", &["1"])], &["pw", "abc", "password"], vec![]);
    let mut form = Form::default();
    form.editing = Some((0, 3));
    let page = layout_all_with(
        &[i],
        30,
        &form,
        LayoutOptions { graphical: Some(controls()), ..LayoutOptions::default() },
    )
    .unwrap();
    assert!(texts(&page.rows)[0].contains("\u{2022}\u{2022}\u{2022} "));
    assert_eq!(page.inputs[0].segs, vec![(0, 4)]);
    assert_eq!(page.rows[0].info.as_ref().unwrap().caret, Some(4));
}

#[test]
fn a_border_in_a_window_is_a_shape_through_blank_gutters() {
    let b = el("box", &[("border", &["1"]), ("border-color", &["red"]), ("border-radius", &["md"]), ("border-fading", &["bottom"])], &[], vec![el("p", &[], &["hi"], vec![])]);
    let page = gfx(&[b], 10);
    assert_eq!(texts(&page.rows)[0].trim(), ""); // the top gutter row is blank
    assert!(texts(&page.rows).iter().all(|t| !t.contains('\u{2502}') && !t.contains('\u{2500}')));
    assert_eq!(page.rows.len(), 3);
    let d = &page.decos[0];
    assert_eq!((d.cols, d.rows), (4, 3));
    assert_eq!(d.stroke, Some(RED));
    assert_eq!(d.fade, Fade::Bottom);
    assert_eq!(d.radius, 8.0);
    assert_eq!(d.sides, [true; 4]);
    assert_eq!(d.fill, None);
}

#[test]
fn a_rounded_box_with_a_background_fills_with_a_shape_not_cells() {
    let b = el("box", &[("bcolor", &["blue"]), ("border-radius", &["lg"]), ("pad", &["1"])], &[], vec![el("p", &[], &["hi"], vec![])]);
    let page = gfx(&[b], 10);
    assert_eq!(page.decos[0].fill, Some(BLUE));
    // The corner cells keep the surrounding background; the text sits on the box's.
    assert!(page.rows[0].spans.iter().all(|s| s.bg.is_none()));
    assert!(page.rows[1].spans.iter().any(|s| s.text == "hi" && s.bg == Some(BLUE)));
}

#[test]
fn shapes_follow_their_element_down_and_across() {
    let inner = el("button", &[], &["ok"], vec![]);
    let outer = el("box", &[("margin", &["1"]), ("pad", &["1"]), ("border", &["1"])], &[], vec![el("p", &[], &["x"], vec![]), inner]);
    let page = gfx(&[el("p", &[], &["top"], vec![]), outer], 20);
    let button = page.decos.iter().find(|d| d.kind == DecoKind::Button).unwrap();
    let row = page.rows.iter().position(|r| row_text(r).contains("ok")).unwrap();
    assert_eq!(button.row, row);
    let col = row_text(&page.rows[row]).find("ok").unwrap();
    assert_eq!(button.col + 1, col);
}

#[test]
fn a_terminal_rounds_corners_with_characters() {
    let b = el("box", &[("border", &["1"]), ("border-radius", &["sm"])], &[], vec![el("p", &[], &["hi"], vec![])]);
    let rows = lay(&[b], 10).rows;
    let t = texts(&rows);
    assert_eq!(t[0].trim_end(), "\u{256D}\u{2500}\u{2500}\u{256E}");
    assert_eq!(t[2].trim_end(), "\u{2570}\u{2500}\u{2500}\u{256F}");
    let plain = el("box", &[("border", &["1"])], &[], vec![el("p", &[], &["hi"], vec![])]);
    assert_eq!(texts(&lay(&[plain], 10).rows)[0].trim_end(), "\u{250C}\u{2500}\u{2500}\u{2510}");
}

#[test]
fn no_shapes_without_a_window() {
    let b = el("button", &[], &["Go"], vec![]);
    let page = lay(&[b], 20);
    assert!(page.decos.is_empty());
    assert!(texts(&page.rows)[0].starts_with("[ Go ]"));
}

#[test]
fn bad_radius_and_fading_are_reported() {
    let b = el("box", &[("border-radius", &["round"])], &[], vec![]);
    assert!(layout_all(&[b], 10, &Form::default()).unwrap_err().contains("border-radius"));
    let b = el("box", &[("border-fading", &["diagonal"])], &[], vec![]);
    assert!(layout_all(&[b], 10, &Form::default()).unwrap_err().contains("border-fading"));
}
