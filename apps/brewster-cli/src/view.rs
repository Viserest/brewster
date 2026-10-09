//! Pure helpers for painting rows (no terminal access, so they are unit-tested).

use engine::{Row, Span};

/// Splits a row into pieces so that cells `[from, to)` can be painted reversed.
/// Every piece keeps the style of the span it came from.
pub fn mark(row: &Row, range: Option<(usize, usize)>) -> Vec<(Span, bool)> {
    let Some((from, to)) = range else {
        return row.spans.iter().map(|s| (s.clone(), false)).collect();
    };
    let mut out: Vec<(Span, bool)> = Vec::new();
    let mut col = 0usize;
    for s in &row.spans {
        let chars: Vec<char> = s.text.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            let inside = col + i >= from && col + i < to;
            let mut j = i + 1;
            while j < chars.len() && (col + j >= from && col + j < to) == inside {
                j += 1;
            }
            out.push((
                Span {
                    text: chars[i..j].iter().collect(),
                    ..s.clone()
                },
                inside,
            ));
            i = j;
        }
        col += chars.len();
    }
    out
}

/// The cells to paint reversed for a row: the whole text of the row when it is under the
/// cursor (one cell for an empty line, so the cursor stays visible), or just the caret
/// cell while an input is being edited.
pub fn reverse_range(
    on_cursor: bool,
    cols: (usize, usize),
    caret: Option<usize>,
    editing: bool,
) -> Option<(usize, usize)> {
    if editing {
        return caret.map(|c| (c, c + 1));
    }
    if on_cursor {
        let (a, b) = cols;
        return Some(if b > a { (a, b) } else { (a, a + 1) });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::Rgb;

    fn span(text: &str, fg: Option<Rgb>) -> Span {
        Span {
            text: text.to_string(),
            fg,
            bg: None,
            bold: false,
            underline: false,
        }
    }

    fn row(spans: Vec<Span>) -> Row {
        Row { spans, info: None }
    }

    fn texts(p: &[(Span, bool)]) -> Vec<(String, bool)> {
        p.iter().map(|(s, r)| (s.text.clone(), *r)).collect()
    }

    #[test]
    fn no_range_keeps_the_spans() {
        let r = row(vec![span("ab", None), span("cd", None)]);
        assert_eq!(
            texts(&mark(&r, None)),
            vec![("ab".to_string(), false), ("cd".to_string(), false)]
        );
    }

    #[test]
    fn range_inside_one_span_splits_it_in_three() {
        let r = row(vec![span("abcdef", None)]);
        assert_eq!(
            texts(&mark(&r, Some((2, 4)))),
            vec![
                ("ab".to_string(), false),
                ("cd".to_string(), true),
                ("ef".to_string(), false)
            ]
        );
    }

    #[test]
    fn range_across_spans_keeps_each_style() {
        let red = Some(Rgb(220, 50, 47));
        let r = row(vec![span("ab", red), span("cd", None)]);
        let p = mark(&r, Some((1, 3)));
        assert_eq!(
            texts(&p),
            vec![
                ("a".to_string(), false),
                ("b".to_string(), true),
                ("c".to_string(), true),
                ("d".to_string(), false)
            ]
        );
        assert_eq!(p[1].0.fg, red);
        assert_eq!(p[2].0.fg, None);
    }

    #[test]
    fn range_outside_the_row_marks_nothing() {
        let r = row(vec![span("ab", None)]);
        assert!(mark(&r, Some((5, 9))).iter().all(|(_, rev)| !rev));
        assert_eq!(mark(&r, Some((5, 9))).len(), 1);
    }

    #[test]
    fn multibyte_text_is_split_by_cell() {
        let r = row(vec![span("héllo", None)]);
        let p = mark(&r, Some((1, 2)));
        assert_eq!(
            texts(&p),
            vec![
                ("h".to_string(), false),
                ("é".to_string(), true),
                ("llo".to_string(), false)
            ]
        );
    }

    #[test]
    fn empty_spans_produce_nothing() {
        let r = row(vec![span("", None), span("a", None)]);
        assert_eq!(texts(&mark(&r, Some((0, 1)))), vec![("a".to_string(), true)]);
    }

    #[test]
    fn reverse_range_rules() {
        assert_eq!(reverse_range(true, (2, 6), None, false), Some((2, 6)));
        assert_eq!(reverse_range(true, (2, 2), None, false), Some((2, 3)));
        assert_eq!(reverse_range(false, (2, 6), None, false), None);
        // While editing only the caret cell is reversed, and only on its row.
        assert_eq!(reverse_range(true, (2, 6), Some(4), true), Some((4, 5)));
        assert_eq!(reverse_range(true, (2, 6), None, true), None);
    }
}
