use super::*;

fn snap(e: &Element) -> tree::Element {
    e.node().snapshot()
}

fn texts(e: &tree::Element, out: &mut Vec<String>) {
    if let Some(t) = e.args.first() {
        out.push(t.clone());
    }
    for c in &e.children {
        texts(c, out);
    }
}

fn all_text(e: &Element) -> Vec<String> {
    let mut v = Vec::new();
    texts(&snap(e), &mut v);
    v
}

#[test]
fn tag_macros_make_the_right_elements() {
    let e = bx!([h1!("T"), p!("body"), pre!("raw"), span!("s"), h6!("six")]);
    let s = snap(&e);
    assert_eq!(s.tag, "box");
    let tags: Vec<&str> = s.children.iter().map(|c| c.tag.as_str()).collect();
    assert_eq!(tags, ["h1", "p", "pre", "span", "h6"]);
    assert_eq!(all_text(&e), ["T", "body", "raw", "s", "six"]);
}

#[test]
fn every_shape_of_bx_works() {
    assert!(snap(&bx!()).children.is_empty());
    assert_eq!(snap(&bx!([p!("a"), p!("b"),])).children.len(), 2);
    let e = bx!({ pad: "1" });
    assert_eq!(snap(&e).style["pad"], ["1"]);
    let e = bx!({ pad: "1", }, []);
    assert_eq!(snap(&e).style["pad"], ["1"]);
    let e = bx!({ id: "x" }, [p!("a")]);
    assert_eq!(snap(&e).children.len(), 1);
}

#[test]
fn attributes_are_parsed_from_strings_into_their_enums() {
    let e = bx!({
        bcolor: "#181818",
        fcolor: "white",
        border_color: "cyan",
        pad: "1,0,1,2",
        margin: "2",
        border: "1,0",
        size: "20,min",
        align_x: "middle",
        id: "main",
    });
    let st = snap(&e).style;
    assert_eq!(st["bcolor"], ["#181818"]);
    assert_eq!(st["fcolor"], ["white"]);
    assert_eq!(st["border-color"], ["cyan"]);
    assert_eq!(st["pad"], ["1", "0", "1", "2"]);
    assert_eq!(st["margin"], ["2"]);
    assert_eq!(st["border"], ["1", "0"]);
    assert_eq!(st["size"], ["20", "min"]);
    assert_eq!(st["align-x"], ["middle"]);
    assert_eq!(st["id"], ["main"]);
}

#[test]
fn typed_values_work_as_well_as_strings() {
    let e = p!({ fcolor: Color::Red, pad: Pad::Xy(1, 2), border: Border::On, id: String::from("i") }, "x");
    let st = snap(&e).style;
    assert_eq!(st["fcolor"], ["red"]);
    assert_eq!(st["pad"], ["1", "2"]);
    assert_eq!(st["border"], ["1"]);
}

#[test]
fn bad_values_are_kept_so_the_browser_can_report_them() {
    let e = p!({ fcolor: "no-such-color", pad: "1,2,3", border: "2" }, "x");
    let st = snap(&e).style;
    assert_eq!(st["fcolor"], ["no-such-color"]);
    assert_eq!(st["pad"], ["1", "2", "3"]);
    assert_eq!(st["border"], ["2"]);
}

#[test]
fn link_and_input_arguments() {
    let l = link!("https://x.y", "Home");
    assert_eq!(snap(&l).args, ["https://x.y", "Home"]);
    let l = link!({ fcolor: "blue" }, "/a", "A");
    assert_eq!(snap(&l).style["fcolor"], ["blue"]);
    let i = input!({ size: "20,min" }, Text, "name", "John");
    let s = snap(&i);
    assert_eq!(s.args, ["name", "John"]);
    assert!(!s.style.contains_key("protected"));
    let i = input!(Password, "pw", "hunter2");
    assert!(snap(&i).style.contains_key("protected"));
}

#[test]
fn extends_copies_style_and_before_after_but_not_children() {
    let t = bx!({ pad: "1", bcolor: "black", before: bx!([p!("top")]), after: bx!([p!("end")]) }, [p!("skipped")]);
    let e = bx!({ extends: &t, pad: "2" }, [p!("kept")]);
    let st = snap(&e).style;
    assert_eq!(st["pad"], ["2"], "own attributes win");
    assert_eq!(st["bcolor"], ["black"]);
    assert_eq!(all_text(&e), ["top", "kept", "end"]);
    // The template was not touched and can be extended again.
    let e2 = bx!({ extends: &t }, [p!("two")]);
    assert_eq!(all_text(&e2), ["top", "two", "end"]);
    assert!(all_text(&t).contains(&"skipped".to_string()));
}

#[test]
fn extends_order_does_not_matter() {
    let t = bx!({ pad: "1" });
    let a = bx!({ extends: &t, pad: "3" });
    let b = bx!({ pad: "3", extends: &t });
    assert_eq!(snap(&a).style["pad"], ["3"]);
    assert_eq!(snap(&b).style["pad"], ["3"]);
}

#[test]
fn before_and_after_wrap_the_children_in_order() {
    let e = bx!({ before: bx!([p!("b1")]), after: bx!([p!("a1")]) }, [p!("mid1"), p!("mid2")]);
    assert_eq!(all_text(&e), ["b1", "mid1", "mid2", "a1"]);
}

#[test]
fn handlers_get_the_clicked_element_and_the_root() {
    let page = LivePage::new(|| {
        bx!([
            p!({ id: "out" }, "waiting"),
            button!({ onclick: |this: &Element, root: &Root| {
                this.set_text("pressed");
                root.tree.query("#out").unwrap().set_text("done");
            } }, "go"),
            button!("plain"),
        ])
    });
    let find = |text: &str| {
        let mut found = None;
        fn walk(e: &tree::Element, text: &str, found: &mut Option<usize>) {
            if e.args.first().map(String::as_str) == Some(text) {
                *found = Some(e.id);
            }
            for c in &e.children {
                walk(c, text, found);
            }
        }
        for e in page.tree() {
            walk(&e, text, &mut found);
        }
        found.unwrap()
    };
    let go = find("go");
    let plain = find("plain");
    assert!(page.is_clickable(go));
    assert!(!page.is_clickable(plain));
    assert!(!page.click(plain));
    assert!(page.click(go));
    let mut t = Vec::new();
    for e in page.tree() {
        texts(&e, &mut t);
    }
    assert_eq!(t, ["done", "pressed", "plain"]);
}

#[test]
fn the_onclick_marker_is_in_the_tree_so_the_host_can_see_it() {
    let b = button!({ onclick: |_: &Element, _: &Root| {} }, "x");
    assert!(snap(&b).style.contains_key("onclick"));
}

#[test]
fn a_clone_of_a_template_keeps_its_handlers() {
    let t = bx!({ before: button!({ onclick: |this: &Element, _: &Root| this.set_text("hit") }, "b") });
    let page = LivePage::new(|| bx!({ extends: &t }));
    let id = page.tree()[0].children[0].id;
    assert!(page.is_clickable(id));
    assert!(page.click(id));
    assert_eq!(page.tree()[0].children[0].args[0], "hit");
}

#[test]
fn children_on_a_leaf_are_recorded_not_fatal() {
    let page = LivePage::new(|| {
        let leaf = p!("x");
        leaf.push(p!("y"));
        bx!([leaf])
    });
    let problems = page.problems();
    assert_eq!(problems.len(), 1, "{:?}", problems);
    assert!(problems[0].contains("cannot have children"));
    assert!(page.problems().is_empty(), "taken once");
}

#[test]
fn set_text_picks_the_text_argument_of_each_tag() {
    let l = link!("/a", "A");
    l.set_text("B");
    assert_eq!(snap(&l).args, ["/a", "B"]);
    let i = input!(Text, "n", "v");
    i.set_text("w");
    assert_eq!(snap(&i).args, ["n", "w"]);
    let p = p!("x");
    p.set_text(String::from("y"));
    assert_eq!(snap(&p).args, ["y"]);
}

#[test]
fn the_wasm_exports_round_trip() {
    fn build() -> Element {
        bx!([button!({ onclick: |this: &Element, _: &Root| this.set_text("hit") }, "b")])
    }
    // Pointers are only 32 bits on wasm32; here the low bits give the length, which is
    // enough to see that a tree came out.
    let packed = abi::page(build);
    let len = (packed & 0xffff_ffff) as usize;
    assert!(len > 8);
    let tree = abi::PAGE_TREE_FOR_TESTS();
    assert_eq!(tree[0].children[0].args[0], "b");
    let id = tree[0].children[0].id as u32;
    assert_eq!(abi::event(id), 1);
    assert_eq!(abi::event(u32::MAX), 0);
    // The host asks for the page again after an event.
    abi::page(build);
    assert_eq!(abi::PAGE_TREE_FOR_TESTS()[0].children[0].args[0], "hit");
    let _ = abi::problems();
}
