//! Engine: the full pipeline (lex -> parse -> resolve -> layout) behind one call.
//! Both apps depend on this crate only.
//!
//! `render_source` is the one-shot path. Interactive apps parse once into a
//! [`Document`] and lay it out again whenever the width or the form state changes.

pub use cursor::edit::{self, EditKey};
pub use cursor::{Cursor, Mode, scroll_into_view};
pub use layout::{Form, InputLayout, Kind, LineInfo, Page, Rgb, Row, Span, row_text};
pub use parser::Stmt;

use resolver::{Resolved, Resolver};

/// A parsed and resolved page, independent of any width.
#[derive(Debug, Clone)]
pub struct Document {
    roots: Vec<Resolved>,
}

impl Document {
    /// Lexes, parses and resolves `src`. `let` statements define templates for the
    /// elements that follow them; top-level elements are stacked in order.
    pub fn parse(src: &str) -> Result<Document, String> {
        let tokens = lexer::lex(src)?;
        let program = parser::parse(tokens)?;
        let mut resolver = Resolver::new();
        let mut roots: Vec<Resolved> = Vec::new();
        for stmt in program {
            match stmt {
                Stmt::Let(name, node) => resolver.define(name, node),
                Stmt::Elem(node) => roots.push(resolver.resolve(&node)?),
            }
        }
        Ok(Document { roots })
    }

    /// Lays the page out in `width` cells with the given input values and caret.
    pub fn layout(&self, width: usize, form: &Form) -> Result<Page, String> {
        layout::layout_all(&self.roots, width, form)
    }
}

/// Renders pseudo source into rows that are `width` cells wide.
pub fn render_source(src: &str, width: usize) -> Result<Vec<Row>, String> {
    Ok(Document::parse(src)?
        .layout(width, &Form::default())?
        .rows)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let wide = doc.layout(20, &Form::default()).unwrap();
        let narrow = doc.layout(4, &Form::default()).unwrap();
        assert_eq!(wide.rows.len(), 1);
        assert_eq!(narrow.rows.len(), 2);
    }

    #[test]
    fn focus_and_input_ids_run_across_top_level_elements() {
        let src = "link https://a.b \"A\"\ninput \"n\" \"v\"\nbutton \"B\"\ninput \"m\" \"w\"";
        let page = Document::parse(src)
            .unwrap()
            .layout(20, &Form::default())
            .unwrap();
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
            vec![
                (Some(0), None),
                (Some(1), Some(0)),
                (Some(2), None),
                (Some(3), Some(1)),
            ]
        );
        assert_eq!(page.inputs.len(), 2);
    }

    #[test]
    fn form_values_reach_the_layout() {
        let doc = Document::parse("input \"name\" \"John\"").unwrap();
        let mut form = Form::default();
        form.values.insert(0, "Jane".to_string());
        let page = doc.layout(30, &form).unwrap();
        assert_eq!(row_text(&page.rows[0]).trim_end(), "name: [Jane]");
    }

    #[test]
    fn headers_and_sizes_work_end_to_end() {
        let rows = render_source("h1 \"Hi\"\np.size:6,2.bcolor:red \"a\"", 10).unwrap();
        let text: Vec<String> = rows.iter().map(row_text).collect();
        assert_eq!(text[0], "=== HI ===");
        assert_eq!(text[1], "a         ");
        assert_eq!(text[2], "          ");
    }
}
