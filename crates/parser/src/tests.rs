use super::*;

fn prog(src: &str) -> Vec<Stmt> {
    parse_source(src).unwrap_or_else(|e| panic!("parse failed: {}\n{}", e, src))
}

fn err(src: &str) -> String {
    parse_source(src).expect_err("expected a parse error")
}

fn first_elem(src: &str) -> ElemNode {
    match prog(src).remove(0).kind {
        StmtKind::Expr { expr: Expr::Element(e), .. } => *e,
        other => panic!("not an element statement: {:?}", other),
    }
}

fn text(s: &str) -> Expr {
    Expr::Str(vec![StrPart::Text(s.to_string())])
}

#[test]
fn element_with_args_and_mods() {
    let e = first_elem("p.fg:red.bold \"Hello\"");
    assert_eq!(e.tag, "p");
    assert_eq!(e.mods, vec![("fg".to_string(), vec!["red".to_string()]), ("bold".to_string(), vec![])]);
    assert_eq!(e.args, vec![text("Hello")]);
    assert!(e.body.is_none());
}

#[test]
fn nested_blocks() {
    let e = first_elem("box.pad:1 {\n  h1 \"T\"\n  p \"a\"\n  p \"b\"\n}");
    let body = e.body.unwrap();
    assert_eq!(body.stmts.len(), 3);
    assert!(matches!(&body.stmts[0].kind, StmtKind::Expr { expr: Expr::Element(h), .. } if h.tag == "h1"));
}

#[test]
fn multi_value_styles() {
    let e = first_elem("box.size:20,5.border:1,2 {}");
    assert_eq!(e.mods[0], ("size".to_string(), vec!["20".to_string(), "5".to_string()]));
    assert_eq!(e.mods[1], ("border".to_string(), vec!["1".to_string(), "2".to_string()]));
    assert!(e.body.unwrap().stmts.is_empty());
}

#[test]
fn hex_colors_are_style_values() {
    let e = first_elem("p.bg:#ff0000 \"x\"");
    assert_eq!(e.mods[0].1, vec!["#ff0000".to_string()]);
}

#[test]
fn template_let_and_extends() {
    let p = prog("let card = box.pad:1 { p \"in\" }\nbox.extends:card { p \"x\" }");
    assert!(matches!(&p[0].kind, StmtKind::Let { name, init: Expr::Element(_), .. } if name == "card"));
    match &p[1].kind {
        StmtKind::Expr { expr: Expr::Element(e), .. } => {
            assert_eq!(e.mods[0], ("extends".to_string(), vec!["card".to_string()]));
        }
        other => panic!("{:?}", other),
    }
}

#[test]
fn before_and_after() {
    let e = first_elem("p.before:span \"> \".after:span \" <\"");
    assert_eq!(e.before.unwrap().args, vec![text("> ")]);
    assert_eq!(e.after.unwrap().args, vec![text(" <")]);
}

#[test]
fn bare_input_name_on_the_same_line() {
    let e = first_elem("input username");
    assert_eq!(e.args, vec![text("username")]);
    // A name on the next line is not swallowed.
    let p = prog("input\nusername");
    assert_eq!(p.len(), 2);
}

#[test]
fn input_with_string_name_and_value() {
    let e = first_elem("input.size:10 \"name\" \"value\"");
    assert_eq!(e.args, vec![text("name"), text("value")]);
}

#[test]
fn onclick_with_typed_args() {
    let e = first_elem("button.onclick:greet(\"hi\", 5, true) \"Go\"");
    assert_eq!(
        e.mods[0],
        (
            "onclick".to_string(),
            vec!["greet".to_string(), "s:hi".to_string(), "n:5".to_string(), "b:true".to_string()]
        )
    );
    assert_eq!(e.args, vec![text("Go")]);
    let e = first_elem("button.onclick:go \"Go\"");
    assert_eq!(e.mods[0].1, vec!["go".to_string()]);
}

#[test]
fn onclick_rejects_non_literals() {
    assert!(err("button.onclick:f(x) \"a\"").contains("literals"));
}

#[test]
fn string_interpolation() {
    let p = prog("p \"a {x} b {{lit}}\"");
    let StmtKind::Expr { expr: Expr::Element(e), .. } = &p[0].kind else { panic!() };
    assert_eq!(
        e.args[0],
        Expr::Str(vec![
            StrPart::Text("a ".into()),
            StrPart::Expr(Expr::Ident("x".into())),
            StrPart::Text(" b {lit}".into()),
        ])
    );
}

#[test]
fn string_interpolation_errors() {
    assert!(err("p \"oops }\"").contains("unmatched"));
    assert!(err("p \"oops {x\"").contains("missing"));
    assert!(err("p \"empty {}\"").contains("empty"));
}

#[test]
fn functions_and_calls() {
    let p = prog("fn add(a: Int, b: Int) -> Int { a + b }\nlet r = add(1, 2)");
    match &p[0].kind {
        StmtKind::Fn(f) => {
            assert_eq!(f.name, "add");
            assert_eq!(f.params, vec!["a".to_string(), "b".to_string()]);
            assert_eq!(f.body.stmts.len(), 1);
        }
        other => panic!("{:?}", other),
    }
    assert!(matches!(&p[1].kind, StmtKind::Let { init: Expr::Call(..), .. }));
}

#[test]
fn operator_precedence() {
    let p = prog("let x = 1 + 2 * 3 == 7 && !false");
    let StmtKind::Let { init, .. } = &p[0].kind else { panic!() };
    // ((1 + (2 * 3)) == 7) && (!false)
    let Expr::Binary(BinOp::And, l, r) = init else { panic!("{:?}", init) };
    assert!(matches!(**r, Expr::Unary(UnOp::Not, _)));
    let Expr::Binary(BinOp::Eq, sum, _) = &**l else { panic!() };
    let Expr::Binary(BinOp::Add, _, prod) = &**sum else { panic!() };
    assert!(matches!(**prod, Expr::Binary(BinOp::Mul, _, _)));
}

#[test]
fn if_else_if_chain() {
    let p = prog("let x = if a { 1 } else if b { 2 } else { 3 }");
    let StmtKind::Let { init: Expr::If(_, _, Some(els)), .. } = &p[0].kind else { panic!() };
    assert!(matches!(&els.stmts[0].kind, StmtKind::Expr { expr: Expr::If(_, _, Some(_)), .. }));
}

#[test]
fn match_with_patterns() {
    let p = prog("let x = match r { Ok(v) => v, Err(_) => 0, }");
    let StmtKind::Let { init: Expr::Match(_, arms), .. } = &p[0].kind else { panic!() };
    assert_eq!(arms.len(), 2);
    assert_eq!(arms[0].0, Pat::Variant("Ok".into(), vec![Pat::Bind("v".into())]));
    assert_eq!(arms[1].0, Pat::Variant("Err".into(), vec![Pat::Wild]));
    let p = prog("let x = match n { 1 => \"a\" \"s\" => \"b\" true => \"c\" _ => \"d\" }");
    let StmtKind::Let { init: Expr::Match(_, arms), .. } = &p[0].kind else { panic!() };
    assert_eq!(arms[0].0, Pat::Int(1));
    assert_eq!(arms[1].0, Pat::Str("s".into()));
    assert_eq!(arms[2].0, Pat::Bool(true));
}

#[test]
fn while_assign_return_and_closures() {
    let p = prog("fn f() { let mut i = 0; while i < 3 { i = i + 1 } return i }");
    let StmtKind::Fn(f) = &p[0].kind else { panic!() };
    assert!(matches!(&f.body.stmts[0].kind, StmtKind::Let { mutable: true, .. }));
    assert!(matches!(&f.body.stmts[1].kind, StmtKind::Expr { expr: Expr::While(..), .. }));
    assert!(matches!(&f.body.stmts[2].kind, StmtKind::Expr { expr: Expr::Return(Some(_)), .. }));
    let p = prog("let f = |a, b| a + b\nlet g = || { 1 }");
    assert!(matches!(&p[0].kind, StmtKind::Let { init: Expr::Closure(ps, _), .. } if ps.len() == 2));
    assert!(matches!(&p[1].kind, StmtKind::Let { init: Expr::Closure(ps, _), .. } if ps.is_empty()));
}

#[test]
fn method_chains_and_paths() {
    let p = prog("root.plugin.install(\"app\", Plugin::source(\"x\"))");
    let StmtKind::Expr { expr: Expr::Call(callee, args), .. } = &p[0].kind else { panic!() };
    assert!(matches!(&**callee, Expr::Field(_, n) if n == "install"));
    assert_eq!(args.len(), 2);
    assert_eq!(args[1], Expr::Call(Box::new(Expr::Path(vec!["Plugin".into(), "source".into()])), vec![text("x")]));
}

#[test]
fn type_alias_is_skipped() {
    let p = prog("type Handler = Fn(msg: String) -> Element\np \"x\"");
    assert!(matches!(p[0].kind, StmtKind::TypeAlias));
    assert_eq!(p.len(), 2);
}

#[test]
fn types_with_generics_are_skipped() {
    let p = prog("fn f(a: List<String>) -> Result<Node, QueryError> { 1 }");
    assert!(matches!(p[0].kind, StmtKind::Fn(_)));
}

#[test]
fn errors_name_the_line() {
    assert!(err("p \"x\"\nbox {").contains("missing `}`"));
    assert!(err("let = 3").contains("line 1"));
    assert!(err("let box = 3").contains("reserved"));
    assert!(err("1 = 2").contains("left side"));
    assert!(err("fn () {}").contains("expected"));
}

#[test]
fn parse_expr_source_rejects_trailing_text() {
    assert!(parse_expr_source("a b", 1).is_err());
    assert_eq!(parse_expr_source("a", 1).unwrap(), Expr::Ident("a".into()));
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    let p = prog("// comment\n\np \"a\" // trailing\n");
    assert_eq!(p.len(), 1);
}

#[test]
fn unit_literal() {
    let p = prog("let r = Ok(())");
    let StmtKind::Let { init: Expr::Call(_, args), .. } = &p[0].kind else { panic!() };
    assert_eq!(args[0], Expr::Unit);
}

#[test]
fn unit_pattern() {
    let p = prog("let x = match r { Ok(()) => 1, _ => 2 }");
    let StmtKind::Let { init: Expr::Match(_, arms), .. } = &p[0].kind else { panic!() };
    assert_eq!(arms[0].0, Pat::Variant("Ok".into(), vec![Pat::Unit]));
}

#[test]
fn absurd_nesting_is_an_error_not_a_crash() {
    let deep = format!("let x = {}1{}", "(".repeat(5000), ")".repeat(5000));
    assert!(err(&deep).contains("too deep"));
    let bangs = format!("let x = {}true", "!".repeat(5000));
    assert!(err(&bangs).contains("too deep"));
    let boxes = format!("{}{}", "box {".repeat(2000), "}".repeat(2000));
    assert!(err(&boxes).contains("too deep"));
    // Ordinary depth is fine.
    let ok = format!("let x = {}1{}", "(".repeat(50), ")".repeat(50));
    prog(&ok);
}

#[test]
fn identifier_arguments_on_the_same_line() {
    let e = first_elem("p text");
    assert_eq!(e.args, vec![Expr::Ident("text".into())]);
    let e = first_elem("h1 title.upper() + \"!\"");
    assert!(matches!(&e.args[0], Expr::Binary(BinOp::Add, l, _) if matches!(**l, Expr::Call(..))));
    // `input name` stays a literal name; a later identifier is a variable.
    let e = first_elem("input name value");
    assert_eq!(e.args, vec![text("name"), Expr::Ident("value".into())]);
    // A name on the next line is a separate statement.
    assert_eq!(prog("p \"a\"\nfoo()").len(), 2);
    // Match arms: an identifier followed by `=>` is the next pattern.
    let p = prog("let x = match n { 1 => p \"one\" other => p \"two\" }");
    let StmtKind::Let { init: Expr::Match(_, arms), .. } = &p[0].kind else { panic!() };
    assert_eq!(arms.len(), 2);
}

#[test]
fn body_after_inner_non_box_belongs_to_the_outer_element() {
    let e = first_elem("box.before:p \"a\".after:p \"b\" { p \"body\" }");
    assert_eq!(e.body.unwrap().stmts.len(), 1);
    assert!(e.before.unwrap().body.is_none());
    assert!(e.after.unwrap().body.is_none());
    // An inner box keeps its own body.
    let e = first_elem("box.before:box { p \"in\" }");
    assert!(e.before.unwrap().body.is_some());
    assert!(e.body.is_none());
}
