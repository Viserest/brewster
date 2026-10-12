use super::*;

use std::path::PathBuf;

fn sample() -> Value {
    let mut v = Value::map();
    v.set("name", Value::str("brew")).unwrap();
    v.set("count", Value::Int(-7)).unwrap();
    v.set("ratio", Value::Float(1.5)).unwrap();
    v.set("whole", Value::Float(2.0)).unwrap();
    v.set("on", Value::Bool(true)).unwrap();
    v.set("tabs.position", Value::str("left")).unwrap();
    v.set("tabs.deep.er.value", Value::Int(3)).unwrap();
    v.set(
        "keybinds.quit",
        Value::List(vec![Value::str("q"), Value::str("ctrl-q")]),
    )
    .unwrap();
    v.set("empty_list", Value::List(vec![])).unwrap();
    v.set("empty_map", Value::map()).unwrap();
    v.set(
        "tricky",
        Value::List(vec![
            Value::str("true"),
            Value::str("12"),
            Value::str("a: b"),
            Value::str("x # y"),
            Value::str(" padded "),
            Value::str(""),
            Value::str("- dash"),
            Value::str("quote \" and \\ slash"),
            Value::str("line\nbreak\ttab"),
            Value::str("héllo ✓ 😀"),
            Value::str("null"),
            Value::str("[1, 2]"),
            Value::str("1.5"),
            Value::str("http://x.y/z"),
        ]),
    )
    .unwrap();
    v.set("odd key.with space", Value::Int(1)).ok();
    v
}

fn roundtrip(f: Format, v: &Value) {
    let text = f.write(v);
    let back = f
        .parse(&text)
        .unwrap_or_else(|e| panic!("{}: {}\n{}", f.name(), e, text));
    assert_eq!(&back, v, "{} round trip\n{}", f.name(), text);
}

#[test]
fn json_round_trip() {
    roundtrip(Format::Json, &sample());
}

#[test]
fn yaml_round_trip() {
    roundtrip(Format::Yaml, &sample());
}

#[test]
fn toml_round_trip() {
    // TOML has no null and no empty-map-only table distinction issues in this sample.
    roundtrip(Format::Toml, &sample());
}

#[test]
fn list_of_maps_round_trips_in_every_format() {
    let mut v = Value::map();
    let mut a = Map::new();
    a.insert("k".into(), Value::Int(1));
    a.insert("s".into(), Value::str("x y"));
    v.set(
        "items",
        Value::List(vec![
            Value::Map(a),
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        ]),
    )
    .unwrap();
    for f in [Format::Json, Format::Yaml, Format::Toml] {
        roundtrip(f, &v);
    }
}

#[test]
fn the_same_settings_in_all_three_formats_parse_equal() {
    let t = "[tabs]\nposition = \"left\" # side\nsidebar_width = 30\n\n[keybinds]\nquit = [\"q\", 'ctrl-q']\n";
    let j = r#"{"tabs": {"position": "left", "sidebar_width": 30}, "keybinds": {"quit": ["q", "ctrl-q"]}}"#;
    let y = "tabs:\n  position: left   # side\n  sidebar_width: 30\nkeybinds:\n  quit:\n    - q\n    - ctrl-q\n";
    let a = Format::Toml.parse(t).unwrap();
    assert_eq!(a, Format::Json.parse(j).unwrap());
    assert_eq!(a, Format::Yaml.parse(y).unwrap());
}

#[test]
fn toml_features() {
    let v = Format::Toml
        .parse("a.b.c = 1\n[x]\ny = { p = 1, q = [1, 2,\n 3] }\n\"quoted key\" = 'lit\\eral'\nbig = 1_000\nf = -2.5e1\n")
        .unwrap();
    assert_eq!(v.get("a.b.c"), Some(&Value::Int(1)));
    assert_eq!(v.get("x.y.p"), Some(&Value::Int(1)));
    assert_eq!(v.get("x.y.q").unwrap().as_list().unwrap().len(), 3);
    assert_eq!(
        v.as_map().unwrap()["x"].as_map().unwrap()["quoted key"],
        Value::str("lit\\eral")
    );
    assert_eq!(v.get("x.big"), Some(&Value::Int(1000)));
    assert_eq!(v.get("x.f"), Some(&Value::Float(-25.0)));
}

#[test]
fn toml_errors() {
    assert!(
        Format::Toml
            .parse("a = 1\na = 2")
            .unwrap_err()
            .contains("duplicate")
    );
    assert!(
        Format::Toml
            .parse("[t]\n[t]")
            .unwrap_err()
            .contains("twice")
    );
    assert!(
        Format::Toml
            .parse("a = hello")
            .unwrap_err()
            .contains("quotes")
    );
    assert!(
        Format::Toml
            .parse("a = 1 b = 2")
            .unwrap_err()
            .contains("unexpected")
    );
    assert!(
        Format::Toml
            .parse("[[x]]")
            .unwrap_err()
            .contains("not supported")
    );
    assert!(
        Format::Toml
            .parse("a = \"open")
            .unwrap_err()
            .contains("unterminated")
    );
    assert!(
        Format::Toml
            .parse("a = 1\n[a]")
            .unwrap_err()
            .contains("not a table")
    );
}

#[test]
fn json_errors() {
    assert!(Format::Json.parse("{").is_err());
    assert!(Format::Json.parse("{\"a\": 1,}").is_err());
    assert!(Format::Json.parse("{\"a\": 1} x").is_err());
    assert!(Format::Json.parse("[1]").unwrap_err().contains("key/value"));
    assert!(
        Format::Json
            .parse("{\"a\": 1, \"a\": 2}")
            .unwrap_err()
            .contains("duplicate")
    );
    let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
    assert!(
        Format::Json
            .parse(&format!("{{\"a\": {}}}", deep))
            .unwrap_err()
            .contains("deeply")
    );
}

#[test]
fn json_unicode_escapes() {
    let v = Format::Json.parse(r#"{"a": "é😀"}"#).unwrap();
    assert_eq!(v.get("a"), Some(&Value::str("é😀")));
    assert!(Format::Json.parse(r#"{"a": "\ud83d"}"#).is_err());
}

#[test]
fn yaml_features() {
    let src = "---\n# comment\nname: 'it''s'\nlist:\n- a\n- b: 1\n  c: 2\nflow: {x: 1, y: [1, \"two\"]}\nnothing:\nurl: http://a.b:80/c\nquoted: \"a # not comment\"\n";
    let v = Format::Yaml.parse(src).unwrap();
    assert_eq!(v.get("name"), Some(&Value::str("it's")));
    let l = v.get("list").unwrap().as_list().unwrap();
    assert_eq!(l[0], Value::str("a"));
    assert_eq!(l[1].get("c"), Some(&Value::Int(2)));
    assert_eq!(
        v.get("flow.y").unwrap().as_list().unwrap()[1],
        Value::str("two")
    );
    assert_eq!(v.get("nothing"), Some(&Value::Null));
    assert_eq!(v.get("url"), Some(&Value::str("http://a.b:80/c")));
    assert_eq!(v.get("quoted"), Some(&Value::str("a # not comment")));
}

#[test]
fn yaml_errors() {
    assert!(Format::Yaml.parse("a: 1\n  b: 2").is_err());
    assert!(
        Format::Yaml
            .parse("a: 1\na: 2")
            .unwrap_err()
            .contains("duplicate")
    );
    assert!(
        Format::Yaml
            .parse("a: |\n  text")
            .unwrap_err()
            .contains("multi-line")
    );
    assert!(
        Format::Yaml
            .parse("a: &x 1")
            .unwrap_err()
            .contains("anchors")
    );
    assert!(
        Format::Yaml
            .parse("a: 1\n---\nb: 2")
            .unwrap_err()
            .contains("documents")
    );
    assert!(Format::Yaml.parse("a: [1, 2").is_err());
    assert!(Format::Yaml.parse("\tkey: 1").is_err());
    assert_eq!(Format::Yaml.parse("").unwrap(), Value::map());
}

#[test]
fn dotted_paths() {
    let mut v = Value::map();
    v.set("a.b.c", Value::Int(1)).unwrap();
    assert_eq!(v.get("a.b.c"), Some(&Value::Int(1)));
    assert!(v.set("a.b.c.d", Value::Int(2)).is_err());
    assert!(v.set("a..b", Value::Int(2)).is_err());
    assert_eq!(v.remove("a.b.c"), Some(Value::Int(1)));
    assert_eq!(v.get("a.b.c"), None);
    let mut base = Value::map();
    base.set("x.y", Value::Int(1)).unwrap();
    base.set("x.z", Value::Int(2)).unwrap();
    let mut over = Value::map();
    over.set("x.y", Value::Int(9)).unwrap();
    base.merge(over);
    assert_eq!(base.get("x.y"), Some(&Value::Int(9)));
    assert_eq!(base.get("x.z"), Some(&Value::Int(2)));
}

#[test]
fn defaults_cover_the_schema() {
    let s = Settings::default();
    for o in OPTIONS {
        assert_eq!(s.get(o.key), Some(&o.default_value()), "{}", o.key);
        assert!(o.check(&o.default_value()).is_ok(), "{}", o.key);
    }
    assert_eq!(s.str("tabs.position"), "top");
    assert!(s.bool("view.fill_window"));
    assert_eq!(s.int("tabs.sidebar_width"), 24);
    assert_eq!(
        s.list("keybinds.link_new_tab"),
        vec!["ctrl-click", "middle-click", "ctrl-enter", "t"]
    );
    assert!(s.changed().as_map().unwrap().is_empty());
}

#[test]
fn schema_keys_are_unique_and_nest_cleanly() {
    let mut seen = std::collections::HashSet::new();
    let mut v = Value::map();
    for o in OPTIONS {
        assert!(seen.insert(o.key), "duplicate {}", o.key);
        v.set(o.key, o.default_value()).unwrap();
    }
}

#[test]
fn set_checks_known_options() {
    let mut s = Settings::default();
    assert!(s.set("tabs.position", Value::str("diagonal")).is_err());
    assert!(s.set("tabs.sidebar_width", Value::Int(1)).is_err());
    assert!(s.set("tabs.show", Value::Bool(true)).is_err());
    assert!(s.set("keybinds.quit", Value::str("q")).is_err());
    s.set("tabs.position", Value::str("left")).unwrap();
    s.set("my.own.key", Value::Int(1)).unwrap();
    assert_eq!(s.str("tabs.position"), "left");
    let ch = s.changed();
    assert_eq!(ch.get("tabs.position"), Some(&Value::str("left")));
    assert_eq!(ch.get("my.own.key"), Some(&Value::Int(1)));
    assert_eq!(ch.as_map().unwrap().len(), 2);
    s.reset("tabs.position");
    s.reset("my.own.key");
    assert!(s.changed().as_map().unwrap().is_empty());
}

#[test]
fn from_value_skips_bad_entries_and_keeps_the_rest() {
    let src = "[tabs]\nposition = \"nowhere\"\nsidebar_width = 30\n[future]\nthing = 1\n";
    let (s, problems) = Settings::from_value(&Format::Toml.parse(src).unwrap());
    assert_eq!(s.str("tabs.position"), "top");
    assert_eq!(s.int("tabs.sidebar_width"), 30);
    assert_eq!(s.get("future.thing"), Some(&Value::Int(1)));
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("tabs.position"));
}

#[test]
fn parse_text_for_the_settings_box() {
    let o = option("view.fill_window").unwrap();
    assert_eq!(o.parse_text(" off ").unwrap(), Value::Bool(false));
    assert!(o.parse_text("maybe").is_err());
    let o = option("tabs.sidebar_width").unwrap();
    assert_eq!(o.parse_text("30").unwrap(), Value::Int(30));
    assert!(o.parse_text("x").is_err());
    assert!(o.parse_text("1000").is_err());
    let o = option("keybinds.quit").unwrap();
    assert_eq!(
        o.parse_text("q, ctrl-q ,").unwrap(),
        Value::List(vec![Value::str("q"), Value::str("ctrl-q")])
    );
}

#[test]
fn search_matches_key_and_description() {
    let keys = |q: &str| search(q).iter().map(|o| o.key).collect::<Vec<_>>();
    assert_eq!(search("").len(), OPTIONS.len());
    let t = keys("tab position");
    assert_eq!(t[0], "tabs.position");
    assert!(keys("SIDEBAR").contains(&"tabs.sidebar_width"));
    // description-only matches come after key matches
    let k = keys("new tab");
    assert_eq!(k[0], "keybinds.new_tab");
    assert!(k.contains(&"general.homepage"));
    assert!(keys("zzzz").is_empty());
}

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "brewster-settings-test-{}-{}",
        std::process::id(),
        name
    ));
    p
}

#[test]
fn files_use_the_format_of_their_extension() {
    let dir = tmp("files");
    let _ = std::fs::remove_dir_all(&dir);
    for ext in ["toml", "json", "yaml", "yml"] {
        let p = dir.join(format!("s.{}", ext));
        write_file(&p, &sample()).unwrap();
        assert_eq!(read_file(&p).unwrap(), sample(), "{}", ext);
        assert!(!dir.join(format!("s.{}.tmp", ext)).exists());
    }
    assert!(write_file(&dir.join("s.txt"), &sample()).is_err());
    assert!(
        read_file(&dir.join("missing.toml"))
            .unwrap_err()
            .contains("cannot read")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
