//! Engine: the full pipeline (lex -> parse -> resolve -> layout) behind one call.
//! Both apps depend on this crate only.

pub use layout::{Rgb, Row, Span, row_text};
pub use parser::Stmt;

use resolver::Resolver;

/// Renders pseudo source into rows that are `width` cells wide.
/// Top-level elements are stacked in order; `let` statements define templates.
pub fn render_source(src: &str, width: usize) -> Result<Vec<Row>, String> {
    let tokens = lexer::lex(src)?;
    let program = parser::parse(tokens)?;
    let mut resolver = Resolver::new();
    let mut rows: Vec<Row> = Vec::new();
    for stmt in program {
        match stmt {
            Stmt::Let(name, node) => {
                resolver.define(name, node);
            }
            Stmt::Elem(node) => {
                let resolved = resolver.resolve(&node)?;
                rows.extend(layout::layout(&resolved, width)?);
            }
        }
    }
    Ok(rows)
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
}
