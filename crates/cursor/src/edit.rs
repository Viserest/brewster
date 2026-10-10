//! Editing the value of one input: typing, deleting, moving the caret and pasting.
//!
//! State lives in [`Form`]: `values` holds edited values and `editing` is
//! `(input index, caret)` with the caret as a char index into the value. After every
//! change the app lays the page out again with the same `Form`, so the page always
//! shows the edited value and the caret.

use layout::{Form, InputLayout, Page};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditKey {
    Insert(char),
    /// Text from the clipboard. Line breaks and tabs become spaces.
    Paste(String),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    /// One wrapped line up / down inside the input.
    Up,
    Down,
}

/// Starts editing `input` with the caret at the end of its value. Returns false when the
/// page has no such input.
pub fn begin(form: &mut Form, page: &Page, input: usize) -> bool {
    let Some(il) = page.inputs.get(input) else {
        return false;
    };
    let len = form
        .values
        .entry(input)
        .or_insert_with(|| il.value.clone())
        .chars()
        .count();
    form.editing = Some((input, len));
    true
}

/// Stops editing. The edited value stays in the form.
pub fn end(form: &mut Form) {
    form.editing = None;
}

/// The input being edited, if any.
pub fn editing(form: &Form) -> Option<usize> {
    form.editing.map(|(i, _)| i)
}

/// The current value of `input`: the edited one, else what the page shows.
pub fn value(form: &Form, page: &Page, input: usize) -> Option<String> {
    form.values
        .get(&input)
        .cloned()
        .or_else(|| page.inputs.get(input).map(|il| il.value.clone()))
}

fn sanitize(s: &str) -> String {
    s.replace("\r\n", " ")
        .chars()
        .map(|c| {
            if matches!(c, '\n' | '\r' | '\t') {
                ' '
            } else {
                c
            }
        })
        .filter(|c| !c.is_control())
        .collect()
}

/// Applies `key` to the input being edited. Returns true when the value changed (a caret
/// move returns false). Does nothing when no input is being edited.
pub fn apply(form: &mut Form, page: &Page, key: EditKey) -> bool {
    let Some((input, caret)) = form.editing else {
        return false;
    };
    let Some(il) = page.inputs.get(input) else {
        return false;
    };
    let mut chars: Vec<char> = form
        .values
        .get(&input)
        .cloned()
        .unwrap_or_else(|| il.value.clone())
        .chars()
        .collect();
    let mut caret = caret.min(chars.len());
    let mut changed = false;

    match key {
        EditKey::Insert(c) => {
            if !c.is_control() {
                chars.insert(caret, c);
                caret += 1;
                changed = true;
            }
        }
        EditKey::Paste(s) => {
            let clean: Vec<char> = sanitize(&s).chars().collect();
            if !clean.is_empty() {
                caret += clean.len();
                let at = caret - clean.len();
                chars.splice(at..at, clean);
                changed = true;
            }
        }
        EditKey::Backspace => {
            if caret > 0 {
                chars.remove(caret - 1);
                caret -= 1;
                changed = true;
            }
        }
        EditKey::Delete => {
            if caret < chars.len() {
                chars.remove(caret);
                changed = true;
            }
        }
        EditKey::Left => caret = caret.saturating_sub(1),
        EditKey::Right => caret = (caret + 1).min(chars.len()),
        EditKey::Home => caret = 0,
        EditKey::End => caret = chars.len(),
        EditKey::Up => caret = vertical(il, caret, chars.len(), false),
        EditKey::Down => caret = vertical(il, caret, chars.len(), true),
    }

    form.values.insert(input, chars.into_iter().collect());
    form.editing = Some((input, caret));
    changed
}

/// Caret one visual line up or down, keeping its column. On the first line "up" goes to
/// the start of the value, on the last line "down" goes to its end.
fn vertical(il: &InputLayout, caret: usize, vlen: usize, down: bool) -> usize {
    let d = il.prefix + caret;
    let Some(i) = il.segs.iter().position(|&(s, e)| d >= s && d < e) else {
        return caret;
    };
    let col = d - il.segs[i].0;
    let j = if down {
        if i + 1 >= il.segs.len() {
            return vlen;
        }
        i + 1
    } else {
        if i == 0 {
            return 0;
        }
        i - 1
    };
    let (s, e) = il.segs[j];
    let d2 = (s + col).min(e.saturating_sub(1)).max(s);
    d2.saturating_sub(il.prefix).min(vlen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::*;
    use layout::{layout_all, row_text};

    /// A form editing input 0 of `input "name" <value>`, caret at the end.
    fn editing_form(value: &str, width: usize, size: &[&str]) -> (Form, Page) {
        let style: Vec<(&str, &[&str])> = if size.is_empty() {
            vec![]
        } else {
            vec![("size", size)]
        };
        let nodes = [el("input", &style, &["name", value], vec![])];
        let page = layout_all(&nodes, width, &Form::default()).unwrap();
        let mut form = Form::default();
        assert!(begin(&mut form, &page, 0));
        (form, page)
    }

    fn val(form: &Form) -> String {
        form.values[&0].clone()
    }

    fn caret(form: &Form) -> usize {
        form.editing.unwrap().1
    }

    #[test]
    fn begin_puts_the_caret_at_the_end() {
        let (form, _) = editing_form("abc", 40, &[]);
        assert_eq!(form.editing, Some((0, 3)));
        assert_eq!(val(&form), "abc");
    }

    #[test]
    fn begin_rejects_unknown_inputs() {
        let page = page(&[el("p", &[], &["x"], vec![])], 10);
        let mut form = Form::default();
        assert!(!begin(&mut form, &page, 0));
        assert_eq!(form.editing, None);
    }

    #[test]
    fn begin_keeps_an_earlier_edit() {
        let (mut form, page) = editing_form("abc", 40, &[]);
        apply(&mut form, &page, EditKey::Insert('!'));
        end(&mut form);
        assert!(begin(&mut form, &page, 0));
        assert_eq!(val(&form), "abc!");
        assert_eq!(caret(&form), 4);
    }

    #[test]
    fn typing_inserts_at_the_caret() {
        let (mut form, page) = editing_form("ac", 40, &[]);
        apply(&mut form, &page, EditKey::Left);
        assert!(apply(&mut form, &page, EditKey::Insert('b')));
        assert_eq!(val(&form), "abc");
        assert_eq!(caret(&form), 2);
    }

    #[test]
    fn control_characters_are_not_inserted() {
        let (mut form, page) = editing_form("a", 40, &[]);
        assert!(!apply(&mut form, &page, EditKey::Insert('\n')));
        assert!(!apply(&mut form, &page, EditKey::Insert('\u{7}')));
        assert_eq!(val(&form), "a");
    }

    #[test]
    fn backspace_and_delete() {
        let (mut form, page) = editing_form("abcd", 40, &[]);
        assert!(apply(&mut form, &page, EditKey::Backspace));
        assert_eq!(val(&form), "abc");
        apply(&mut form, &page, EditKey::Home);
        assert!(!apply(&mut form, &page, EditKey::Backspace)); // nothing before the caret
        assert!(apply(&mut form, &page, EditKey::Delete));
        assert_eq!(val(&form), "bc");
        apply(&mut form, &page, EditKey::End);
        assert!(!apply(&mut form, &page, EditKey::Delete)); // nothing after the caret
    }

    #[test]
    fn caret_moves_stay_in_range() {
        let (mut form, page) = editing_form("ab", 40, &[]);
        apply(&mut form, &page, EditKey::Right);
        assert_eq!(caret(&form), 2);
        apply(&mut form, &page, EditKey::Home);
        apply(&mut form, &page, EditKey::Left);
        assert_eq!(caret(&form), 0);
        apply(&mut form, &page, EditKey::End);
        assert_eq!(caret(&form), 2);
        assert_eq!(val(&form), "ab");
    }

    #[test]
    fn multibyte_text_is_edited_by_char() {
        let (mut form, page) = editing_form("héllo", 40, &[]);
        apply(&mut form, &page, EditKey::Home);
        apply(&mut form, &page, EditKey::Right);
        apply(&mut form, &page, EditKey::Right);
        assert!(apply(&mut form, &page, EditKey::Backspace));
        assert_eq!(val(&form), "hllo");
        apply(&mut form, &page, EditKey::Insert('ü'));
        assert_eq!(val(&form), "hüllo");
    }

    #[test]
    fn paste_inserts_at_the_caret() {
        let (mut form, page) = editing_form("ad", 40, &[]);
        apply(&mut form, &page, EditKey::Left);
        assert!(apply(&mut form, &page, EditKey::Paste("bc".to_string())));
        assert_eq!(val(&form), "abcd");
        assert_eq!(caret(&form), 3);
    }

    #[test]
    fn paste_flattens_line_breaks_and_drops_controls() {
        let (mut form, page) = editing_form("", 40, &[]);
        apply(
            &mut form,
            &page,
            EditKey::Paste("a\r\nb\nc\td\u{1b}e".to_string()),
        );
        assert_eq!(val(&form), "a b c de");
    }

    #[test]
    fn empty_paste_changes_nothing() {
        let (mut form, page) = editing_form("x", 40, &[]);
        assert!(!apply(&mut form, &page, EditKey::Paste(String::new())));
        assert!(!apply(
            &mut form,
            &page,
            EditKey::Paste("\u{1b}".to_string())
        ));
        assert_eq!(val(&form), "x");
    }

    #[test]
    fn nothing_happens_when_not_editing() {
        let page = page(&[el("input", &[], &["n", "v"], vec![])], 20);
        let mut form = Form::default();
        assert!(!apply(&mut form, &page, EditKey::Insert('x')));
        assert!(form.values.is_empty());
    }

    #[test]
    fn up_and_down_move_between_wrapped_lines() {
        // Displayed: "name: " / "[hello " / "world foo]" at width 10.
        let (mut form, page) = editing_form("hello world foo", 10, &["10"]);
        assert_eq!(page.inputs[0].segs, vec![(0, 6), (6, 13), (13, 23)]);
        // Caret 8 is displayed index 7 + 8 = 15: line 3 (13..23), column 2.
        form.editing = Some((0, 8));
        apply(&mut form, &page, EditKey::Up);
        // Line 2 is 6..13; column 2 is displayed index 8, which is value index 1.
        assert_eq!(caret(&form), 1);
        apply(&mut form, &page, EditKey::Down);
        assert_eq!(caret(&form), 8);
        // Landing inside the `name: [` prefix clamps to the start of the value.
        form.editing = Some((0, 0));
        apply(&mut form, &page, EditKey::Up);
        assert_eq!(caret(&form), 0);
        assert_eq!(val(&form), "hello world foo");
    }

    #[test]
    fn up_on_the_first_line_goes_home_and_down_on_the_last_goes_to_the_end() {
        // Displayed "name: [aaaa " / "bbbb cccc " / "dddd]" at width 12.
        let (mut form, page) = editing_form("aaaa bbbb cccc dddd", 12, &["12"]);
        assert_eq!(page.inputs[0].segs, vec![(0, 12), (12, 22), (22, 27)]);
        form.editing = Some((0, 2)); // first line
        apply(&mut form, &page, EditKey::Up);
        assert_eq!(caret(&form), 0);
        form.editing = Some((0, 2));
        apply(&mut form, &page, EditKey::Down); // column 9 -> displayed 21 -> value index 14
        assert_eq!(caret(&form), 14);
        form.editing = Some((0, 16)); // last line
        apply(&mut form, &page, EditKey::Down);
        assert_eq!(caret(&form), 19);
    }

    #[test]
    fn the_page_shows_the_edit_and_the_caret() {
        let (mut form, page) = editing_form("abc", 40, &[]);
        apply(&mut form, &page, EditKey::Left);
        apply(&mut form, &page, EditKey::Insert('X'));
        let nodes = [el("input", &[], &["name", "abc"], vec![])];
        let page = layout_all(&nodes, 40, &form).unwrap();
        assert_eq!(row_text(&page.rows[0]).trim_end(), "name: [abXc]");
        // `name: [` is 7 chars; the caret sits after "abX".
        assert_eq!(page.rows[0].info.as_ref().unwrap().caret, Some(10));
    }

    #[test]
    fn value_prefers_the_edit_over_the_page() {
        let (mut form, page) = editing_form("abc", 40, &[]);
        assert_eq!(value(&form, &page, 0).as_deref(), Some("abc"));
        apply(&mut form, &page, EditKey::Insert('d'));
        assert_eq!(value(&form, &page, 0).as_deref(), Some("abcd"));
        assert_eq!(value(&form, &page, 9), None);
        assert_eq!(editing(&form), Some(0));
        end(&mut form);
        assert_eq!(editing(&form), None);
    }
}
