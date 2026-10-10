use super::*;
use resolver::Style;

fn el(tag: &str, style: &[(&str, &[&str])], args: &[&str], children: Vec<Resolved>) -> Resolved {
    let mut s = Style::new();
    for (k, v) in style {
        s.insert(k.to_string(), v.iter().map(|x| x.to_string()).collect());
    }
    Resolved {
        tag: tag.to_string(),
        style: s,
        args: args.iter().map(|x| x.to_string()).collect(),
        children,
    }
}

fn texts(rows: &[Row]) -> Vec<String> {
    rows.iter().map(row_text).collect()
}

fn lay(roots: &[Resolved], width: usize) -> Page {
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
fn h1_is_uppercase_with_three_marks() {
    let s = header("h1");
    assert_eq!(s.text, "=== HI THERE ===");
    assert!(!s.bold && !s.underline);
}

#[test]
fn h2_is_uppercase() {
    let s = header("h2");
    assert_eq!(s.text, "= HI THERE =");
    assert!(!s.bold && !s.underline);
}

#[test]
fn h3_is_bold_and_uppercase() {
    let s = header("h3");
    assert_eq!(s.text, "= HI THERE =");
    assert!(s.bold && !s.underline);
}

#[test]
fn h4_is_bold() {
    let s = header("h4");
    assert_eq!(s.text, "= Hi there =");
    assert!(s.bold && !s.underline);
}

#[test]
fn h5_is_bold_and_underlined() {
    let s = header("h5");
    assert_eq!(s.text, "= Hi there =");
    assert!(s.bold && s.underline);
}

#[test]
fn h6_is_underlined() {
    let s = header("h6");
    assert_eq!(s.text, "= Hi there =");
    assert!(!s.bold && s.underline);
}

#[test]
fn header_marks_are_not_part_of_the_copy_text() {
    let h = el("h1", &[], &["Hi"], vec![]);
    let rows = lay(&[h], 30).rows;
    assert_eq!(rows[0].info.as_ref().unwrap().text, "HI");
}

#[test]
fn header_wraps_inside_its_marks() {
    let h = el("h4", &[], &["aaa bbb ccc"], vec![]);
    let rows = lay(&[h], 9).rows;
    // Width 9 leaves 5 cells between the marks, so each word gets its own line.
    assert_eq!(
        texts(&rows),
        vec![
            "= aaa =  ".to_string(),
            "= bbb =  ".to_string(),
            "= ccc =  ".to_string()
        ]
    );
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
