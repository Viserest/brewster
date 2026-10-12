use super::*;

use std::{cell::RefCell, collections::HashMap, path::PathBuf};

fn u(s: &str) -> Url {
    Url::parse(s).unwrap()
}

/// A loader backed by a map; removing a URL simulates a lost connection.
#[derive(Default)]
struct MapLoader(RefCell<HashMap<String, Vec<u8>>>);

impl MapLoader {
    fn set(&self, url: &str, body: &[u8]) {
        self.0.borrow_mut().insert(url.to_string(), body.to_vec());
    }
    fn drop_all(&self) {
        self.0.borrow_mut().clear();
    }
}

impl Loader for MapLoader {
    fn load(&self, url: &Url) -> Result<String, String> {
        self.load_bytes(url)
            .and_then(|b| String::from_utf8(b).map_err(|e| e.to_string()))
    }
    fn load_bytes(&self, url: &Url) -> Result<Vec<u8>, String> {
        self.0
            .borrow()
            .get(&url.to_string())
            .cloned()
            .ok_or_else(|| "offline".to_string())
    }
}

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("brewster-plugin-test-{}-{}", name, Uuid::new_v4()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn key(a: &str, p: &str) -> AppKey {
    AppKey::new(a, p).unwrap()
}

#[test]
fn uuid_is_v4_and_round_trips() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    assert_ne!(a, b);
    let s = a.to_string();
    assert_eq!(s.len(), 36);
    assert_eq!(&s[14..15], "4");
    assert!(matches!(&s[19..20], "8" | "9" | "a" | "b"));
    assert_eq!(Uuid::parse(&s), Some(a));
    assert_eq!(Uuid::parse("nope"), None);
    assert_eq!(Uuid::parse(&s.replace('-', "x")), None);
}

#[test]
fn app_keys_normalise_and_match() {
    assert_eq!(key("a.com", "shop").to_string(), "a.com/shop");
    assert_eq!(key("a.com", "/shop/").to_string(), "a.com/shop");
    assert_eq!(key("a.com", "a.com/shop").to_string(), "a.com/shop");
    assert_eq!(key("a.com", "").to_string(), "a.com");
    assert_eq!(key("a.com", "/").path(), "/");
    assert!(AppKey::new("a.com", "../x").is_err());
    assert!(AppKey::new("a.com", "x?y").is_err());

    let shop = key("a.com", "shop");
    assert!(shop.matches(&u("http://a.com/shop")));
    assert!(shop.matches(&u("http://a.com/shop/cart/1")));
    assert!(!shop.matches(&u("http://a.com/shopping")));
    assert!(!shop.matches(&u("http://b.com/shop")));
    assert!(key("a.com", "").matches(&u("http://a.com/anything")));
    assert!(key("", "docs").matches(&u("file:///docs/x.cre")));
}

#[test]
fn compile_and_validate() {
    let p = Plugin::compile("fn f() { 1 }").unwrap();
    assert!(matches!(&p, Plugin::Compiled(b) if b.starts_with(b"CREB")));
    assert!(p.local_program().unwrap().is_ok());
    assert!(Plugin::compile("fn (").is_err());
    assert!(Plugin::Source("p \"x\"".into()).validate().is_ok());
    assert!(matches!(
        Plugin::Source("let = ".into()).validate(),
        Err(PluginError::Compile(_))
    ));
    assert!(matches!(
        Plugin::Compiled(b"junk".to_vec()).validate(),
        Err(PluginError::Compile(_))
    ));
    assert!(matches!(
        Plugin::Url(" ".into()).validate(),
        Err(PluginError::Fetch(_))
    ));
}

#[test]
fn program_from_bytes_takes_text_or_compiled() {
    assert_eq!(program_from_bytes(b"p \"x\"").unwrap().len(), 1);
    let Plugin::Compiled(b) = Plugin::compile("p \"x\"").unwrap() else {
        panic!()
    };
    assert_eq!(program_from_bytes(&b).unwrap().len(), 1);
    assert!(program_from_bytes(&[0xff, 0xfe]).is_err());
}

#[test]
fn install_list_update_delete_clear() {
    let l = MapLoader::default();
    let base = u("http://a.com/shop");
    let mut r = Registry::in_memory();
    let k = key("a.com", "shop");
    let a = r
        .install(k.clone(), Plugin::Source("fn a() { 1 }".into()), &base, &l)
        .unwrap();
    let b = r
        .install(k.clone(), Plugin::Source("fn b() { 2 }".into()), &base, &l)
        .unwrap();
    r.install(
        key("a.com", "other"),
        Plugin::Source("fn c() { 3 }".into()),
        &base,
        &l,
    )
    .unwrap();
    assert_eq!(r.list(&k).len(), 2);
    assert_eq!(r.list(&k)[0], Plugin::Source("fn a() { 1 }".into()));
    assert_eq!(r.applications().len(), 2);

    r.update(a, Plugin::Source("fn a2() { 1 }".into()), &base, &l)
        .unwrap();
    assert_eq!(r.list(&k)[0], Plugin::Source("fn a2() { 1 }".into()));
    assert_eq!(
        r.update(Uuid::new_v4(), Plugin::Source("".into()), &base, &l)
            .unwrap_err()
            .kind(),
        "NotFound"
    );

    r.delete(b).unwrap();
    assert_eq!(r.list(&k).len(), 1);
    assert_eq!(r.delete(b).unwrap_err(), PluginError::NotFound(b));

    r.clear(&k).unwrap();
    assert!(r.list(&k).is_empty());
    assert_eq!(r.len(), 1);
    r.clear(&k).unwrap(); // clearing nothing is fine
}

#[test]
fn install_rejects_bad_code_without_saving() {
    let l = MapLoader::default();
    let mut r = Registry::in_memory();
    let e = r.install(
        key("a.com", ""),
        Plugin::Source("fn (".into()),
        &u("http://a.com/"),
        &l,
    );
    assert!(matches!(e, Err(PluginError::Compile(_))));
    assert!(r.is_empty());
}

#[test]
fn programs_run_in_insertion_order_for_matching_pages_only() {
    let l = MapLoader::default();
    let base = u("http://a.com/shop");
    let mut r = Registry::in_memory();
    r.install(
        key("a.com", "shop"),
        Plugin::Source("fn first() { 1 }".into()),
        &base,
        &l,
    )
    .unwrap();
    r.install(
        key("a.com", ""),
        Plugin::Source("fn second() { 1 }".into()),
        &base,
        &l,
    )
    .unwrap();
    r.install(
        key("b.com", ""),
        Plugin::Source("fn other() { 1 }".into()),
        &base,
        &l,
    )
    .unwrap();
    r.install(
        key("a.com", "shop"),
        Plugin::Source("fn third() { 1 }".into()),
        &base,
        &l,
    )
    .unwrap();
    let names: Vec<String> = r
        .programs_for(&u("http://a.com/shop/x"), &l)
        .into_iter()
        .map(|(_, p)| match &p.unwrap()[0].kind {
            parser::StmtKind::Fn(f) => f.name.clone(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(names, vec!["first", "second", "third"]);
    assert_eq!(r.programs_for(&u("http://a.com/blog"), &l).len(), 1);
}

#[test]
fn url_plugins_are_cached_and_survive_a_lost_connection() {
    let l = MapLoader::default();
    l.set("http://a.com/p.cre", b"fn remote() { 1 }");
    let base = u("http://a.com/page");
    let mut r = Registry::in_memory();
    let id = r
        .install(key("a.com", ""), Plugin::Url("/p.cre".into()), &base, &l)
        .unwrap();
    // The stored URL is absolute.
    assert_eq!(
        r.get(id).unwrap().plugin,
        Plugin::Url("http://a.com/p.cre".into())
    );
    l.drop_all();
    let progs = r.programs_for(&base, &l);
    assert!(progs[0].1.is_ok(), "cache must be used offline");
    // New content is picked up and cached when the network returns.
    l.set("http://a.com/p.cre", b"fn newer() { 1 }");
    let progs = r.programs_for(&base, &l);
    assert!(
        matches!(&progs[0].1.as_ref().unwrap()[0].kind, parser::StmtKind::Fn(f) if f.name == "newer")
    );
    l.drop_all();
    let progs = r.programs_for(&base, &l);
    assert!(
        matches!(&progs[0].1.as_ref().unwrap()[0].kind, parser::StmtKind::Fn(f) if f.name == "newer")
    );
}

#[test]
fn url_plugin_installed_offline_fails_only_until_fetched() {
    let l = MapLoader::default();
    let base = u("http://a.com/");
    let mut r = Registry::in_memory();
    r.install(key("a.com", ""), Plugin::Url("/p.cre".into()), &base, &l)
        .unwrap();
    assert!(matches!(
        r.programs_for(&base, &l)[0].1,
        Err(PluginError::Fetch(_))
    ));
    l.set("http://a.com/p.cre", b"fn ok() { 1 }");
    assert!(r.programs_for(&base, &l)[0].1.is_ok());
}

#[test]
fn compiled_url_plugins_work() {
    let l = MapLoader::default();
    let Plugin::Compiled(bytes) = Plugin::compile("fn bin() { 1 }").unwrap() else {
        panic!()
    };
    l.set("http://a.com/p.crb", &bytes);
    let base = u("http://a.com/");
    let mut r = Registry::in_memory();
    r.install(key("a.com", ""), Plugin::Url("p.crb".into()), &base, &l)
        .unwrap();
    assert!(r.programs_for(&base, &l)[0].1.is_ok());
}

#[test]
fn url_plugin_with_broken_code_is_rejected_at_install() {
    let l = MapLoader::default();
    l.set("http://a.com/p.cre", b"fn (");
    let mut r = Registry::in_memory();
    let e = r.install(
        key("a.com", ""),
        Plugin::Url("/p.cre".into()),
        &u("http://a.com/"),
        &l,
    );
    assert!(matches!(e, Err(PluginError::Compile(_))));
}

#[test]
fn plugins_are_saved_and_reloaded_in_order() {
    let dir = tmpdir("persist");
    let l = MapLoader::default();
    l.set("http://a.com/p.cre", b"fn remote() { 1 }");
    let base = u("http://a.com/");
    let k = key("a.com", "app");
    let (a, b, c, d);
    {
        let mut r = Registry::open(&dir).unwrap();
        a = r
            .install(
                k.clone(),
                Plugin::Source("fn a() { 1 }\n\nfn z() { 2 }\n".into()),
                &base,
                &l,
            )
            .unwrap();
        b = r
            .install(
                k.clone(),
                Plugin::compile("fn b() { 1 }").unwrap(),
                &base,
                &l,
            )
            .unwrap();
        c = r
            .install(k.clone(), Plugin::Url("/p.cre".into()), &base, &l)
            .unwrap();
        d = r
            .install(
                key("a.com", "gone"),
                Plugin::Source("fn d() { 1 }".into()),
                &base,
                &l,
            )
            .unwrap();
        r.delete(d).unwrap();
    }
    l.drop_all();
    let mut r = Registry::open(&dir).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(r.len(), 3);
    assert!(r.get(d).is_none());
    // Source with blank lines survives byte for byte.
    assert_eq!(
        r.get(a).unwrap().plugin,
        Plugin::Source("fn a() { 1 }\n\nfn z() { 2 }\n".into())
    );
    assert!(matches!(r.get(b).unwrap().plugin, Plugin::Compiled(_)));
    // Offline reload still runs all three, in order, the URL one from its cache.
    let progs = r.programs_for(&u("http://a.com/app/x"), &l);
    assert_eq!(progs.iter().map(|p| p.0).collect::<Vec<_>>(), vec![a, b, c]);
    assert!(progs.iter().all(|p| p.1.is_ok()));
    // New installs continue the order.
    let e = r
        .install(k.clone(), Plugin::Source("fn e() { 1 }".into()), &base, &l)
        .unwrap();
    let ids: Vec<_> = r
        .programs_for(&u("http://a.com/app"), &l)
        .into_iter()
        .map(|p| p.0)
        .collect();
    assert_eq!(ids, vec![a, b, c, e]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_and_clear_change_the_files() {
    let dir = tmpdir("files");
    let l = MapLoader::default();
    let base = u("http://a.com/");
    let k = key("a.com", "");
    let mut r = Registry::open(&dir).unwrap();
    let a = r
        .install(k.clone(), Plugin::Source("fn a() { 1 }".into()), &base, &l)
        .unwrap();
    r.update(a, Plugin::Source("fn a2() { 1 }".into()), &base, &l)
        .unwrap();
    drop(r);
    let r = Registry::open(&dir).unwrap();
    assert_eq!(
        r.get(a).unwrap().plugin,
        Plugin::Source("fn a2() { 1 }".into())
    );
    let mut r = r;
    r.clear(&k).unwrap();
    drop(r);
    assert!(Registry::open(&dir).unwrap().is_empty());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_files_become_warnings_not_failures() {
    let dir = tmpdir("corrupt");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("junk.plugin"), "hello").unwrap();
    std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
    let r = Registry::open(&dir).unwrap();
    assert!(r.is_empty());
    assert_eq!(r.warnings.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
