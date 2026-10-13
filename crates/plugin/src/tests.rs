use super::*;

use std::{cell::RefCell, collections::HashMap, path::PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
    assert!(key("", "docs").matches(&u("file:///docs/x.rs")));
}

/// A stand-in for rustc: `--version` answers, a source containing BROKEN fails, anything
/// else "compiles" to the wasm header followed by the source. Every real run is logged.
#[cfg(unix)]
fn fake_compiler(name: &str) -> (Compiler, PathBuf) {
    let dir = tmpdir(name);
    std::fs::create_dir_all(dir.join("sdk/deps")).unwrap();
    std::fs::write(dir.join("sdk/libcre.rlib"), b"sdk").unwrap();
    let script = dir.join("fake-rustc");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then echo 'rustc 0.0.0-fake'; exit 0; fi\n\
             out=''; src=''\n\
             while [ $# -gt 0 ]; do case \"$1\" in -o) out=\"$2\"; shift;; *.rs) src=\"$1\";; esac; shift; done\n\
             echo run >> '{}'\n\
             if grep -q BROKEN \"$src\"; then echo 'error: BROKEN' >&2; exit 1; fi\n\
             printf '\\0asm' > \"$out\"; cat \"$src\" >> \"$out\"\n",
            dir.join("runs.log").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let c = Compiler::new(&script, Some(dir.join("sdk")), dir.join("cache"));
    (c, dir)
}

#[cfg(unix)]
fn runs(dir: &std::path::Path) -> usize {
    std::fs::read_to_string(dir.join("runs.log")).map_or(0, |s| s.lines().count())
}

fn module_of(src: &str) -> Vec<u8> {
    let mut v = WASM_MAGIC.to_vec();
    v.extend_from_slice(src.as_bytes());
    v
}

#[test]
fn validate_checks_what_it_can_without_compiling() {
    assert!(Plugin::Source("fn page() {}".into()).validate().is_ok());
    assert!(Plugin::Wasm(module_of("x")).validate().is_ok());
    assert!(matches!(
        Plugin::Source(" ".into()).validate(),
        Err(PluginError::Compile(_))
    ));
    assert!(matches!(
        Plugin::Wasm(b"junk".to_vec()).validate(),
        Err(PluginError::Compile(_))
    ));
    assert!(matches!(
        Plugin::Url(" ".into()).validate(),
        Err(PluginError::Fetch(_))
    ));
}

#[test]
fn wasm_sibling_swaps_the_extension() {
    let s = |x: &str| wasm_sibling(&u(x)).map(|u| u.to_string());
    assert_eq!(s("http://a.com/x/demo.rs").as_deref(), Some("http://a.com/x/demo.wasm"));
    assert_eq!(s("file:///p/page").as_deref(), Some("file:///p/page.wasm"));
    assert_eq!(s("http://a.com/"), None);
    assert_eq!(s("http://a.com/a.wasm.wasm"), None);
}

#[cfg(unix)]
#[test]
fn source_is_compiled_once_and_cached_by_content() {
    let (c, dir) = fake_compiler("cache");
    let a = c.compile("page one").unwrap();
    assert_eq!(a, module_of("page one"));
    assert_eq!(c.compile("page one").unwrap(), a);
    assert_eq!(runs(&dir), 1, "second call comes from the cache");
    c.compile("page two").unwrap();
    assert_eq!(runs(&dir), 2, "different source, different key");
    // A fresh compiler (a new run of the program) finds the cache on disk.
    let c2 = Compiler::new(&c.rustc, c.sdk_dir.clone(), c.cache_dir.clone());
    assert_eq!(c2.compile("page one").unwrap(), a);
    assert_eq!(runs(&dir), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_failed_compile_is_tried_once_per_session() {
    let (c, dir) = fake_compiler("fail");
    let e = c.compile("BROKEN").unwrap_err();
    assert!(e.contains("BROKEN"), "{e}");
    assert_eq!(c.compile("BROKEN").unwrap_err(), e);
    assert_eq!(runs(&dir), 1);
    // No leftovers next to the cache.
    let left: Vec<_> = std::fs::read_dir(dir.join("cache")).unwrap().flatten().collect();
    assert!(left.is_empty(), "{:?}", left);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compiling_without_an_sdk_or_rustc_is_an_error_not_a_panic() {
    let c = Compiler::new("rustc", None, tmpdir("nosdk"));
    assert!(c.compile("x").unwrap_err().contains("SDK"));
    let dir = tmpdir("norustc");
    std::fs::create_dir_all(dir.join("sdk")).unwrap();
    std::fs::write(dir.join("sdk/libcre.rlib"), b"x").unwrap();
    let c = Compiler::new(dir.join("missing-rustc"), Some(dir.join("sdk")), dir.join("c"));
    assert!(c.compile("x").unwrap_err().contains("cannot run"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn load_module_takes_wasm_as_is_and_compiles_source() {
    let (c, dir) = fake_compiler("load");
    let l = MapLoader::default();
    l.set("http://a.com/w.wasm", &module_of("ready"));
    l.set("http://a.com/s.rs", b"page src");
    assert_eq!(load_module(&u("http://a.com/w.wasm"), &l, &c).unwrap(), module_of("ready"));
    assert_eq!(runs(&dir), 0);
    assert_eq!(load_module(&u("http://a.com/s.rs"), &l, &c).unwrap(), module_of("page src"));
    assert!(matches!(
        load_module(&u("http://a.com/none.rs"), &l, &c),
        Err(PluginError::Fetch(_))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn source_that_will_not_compile_falls_back_to_the_precompiled_module() {
    let (c, dir) = fake_compiler("fallback");
    let l = MapLoader::default();
    l.set("http://a.com/x/p.rs", b"BROKEN");
    // Nothing pre-compiled: the compile error is reported, with what was tried.
    let e = load_module(&u("http://a.com/x/p.rs"), &l, &c).unwrap_err();
    let PluginError::Compile(m) = e else { panic!() };
    assert!(m.contains("BROKEN") && m.contains("p.wasm"), "{m}");
    // A pre-compiled module beside it is used.
    l.set("http://a.com/x/p.wasm", &module_of("prebuilt"));
    assert_eq!(
        load_module(&u("http://a.com/x/p.rs"), &l, &c).unwrap(),
        module_of("prebuilt")
    );
    // Something that is not a module is not accepted.
    l.set("http://a.com/x/p.wasm", b"html");
    assert!(load_module(&u("http://a.com/x/p.rs"), &l, &c).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_compiler_at_all_still_gets_the_precompiled_module() {
    let c = Compiler::new("rustc", None, tmpdir("nocompiler"));
    let l = MapLoader::default();
    l.set("http://a.com/p.rs", b"fn page() {}");
    l.set("http://a.com/p.wasm", &module_of("prebuilt"));
    assert_eq!(load_module(&u("http://a.com/p.rs"), &l, &c).unwrap(), module_of("prebuilt"));
}

fn no_compiler() -> Compiler {
    Compiler::new("rustc", None, tmpdir("none"))
}

#[test]
fn install_list_update_delete_clear() {
    let l = MapLoader::default();
    let base = u("http://a.com/shop");
    let mut r = Registry::in_memory();
    let k = key("a.com", "shop");
    let a = r.install(k.clone(), Plugin::Source("a".into()), &base, &l).unwrap();
    let b = r.install(k.clone(), Plugin::Source("b".into()), &base, &l).unwrap();
    r.install(key("a.com", "other"), Plugin::Source("c".into()), &base, &l).unwrap();
    assert_eq!(r.list(&k).len(), 2);
    assert_eq!(r.list(&k)[0], Plugin::Source("a".into()));
    assert_eq!(r.applications().len(), 2);

    r.update(a, Plugin::Source("a2".into()), &base, &l).unwrap();
    assert_eq!(r.list(&k)[0], Plugin::Source("a2".into()));
    assert_eq!(
        r.update(Uuid::new_v4(), Plugin::Source("x".into()), &base, &l).unwrap_err().kind(),
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
fn install_rejects_bad_plugins_without_saving() {
    let l = MapLoader::default();
    let mut r = Registry::in_memory();
    let base = u("http://a.com/");
    assert!(r.install(key("a.com", ""), Plugin::Source("".into()), &base, &l).is_err());
    assert!(r.install(key("a.com", ""), Plugin::Wasm(b"junk".to_vec()), &base, &l).is_err());
    assert!(r.is_empty());
}

#[cfg(unix)]
#[test]
fn modules_run_in_insertion_order_for_matching_pages_only() {
    let (c, dir) = fake_compiler("order");
    let l = MapLoader::default();
    let base = u("http://a.com/shop");
    let mut r = Registry::in_memory();
    for (k, src) in [
        (key("a.com", "shop"), "first"),
        (key("a.com", ""), "second"),
        (key("b.com", ""), "other"),
        (key("a.com", "shop"), "third"),
    ] {
        r.install(k, Plugin::Source(src.into()), &base, &l).unwrap();
    }
    let mods: Vec<Vec<u8>> = r
        .modules_for(&u("http://a.com/shop/x"), &l, &c)
        .into_iter()
        .map(|(_, m)| m.unwrap())
        .collect();
    assert_eq!(mods, vec![module_of("first"), module_of("second"), module_of("third")]);
    assert_eq!(r.modules_for(&u("http://a.com/blog"), &l, &c).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_broken_plugin_is_an_error_for_that_plugin_only() {
    let l = MapLoader::default();
    let base = u("http://a.com/");
    let mut r = Registry::in_memory();
    r.install(key("a.com", ""), Plugin::Source("x".into()), &base, &l).unwrap();
    r.install(key("a.com", ""), Plugin::Wasm(module_of("ok")), &base, &l).unwrap();
    let c = no_compiler();
    let m = r.modules_for(&base, &l, &c);
    assert!(matches!(m[0].1, Err(PluginError::Compile(_))));
    assert_eq!(m[1].1.as_ref().unwrap(), &module_of("ok"));
}

#[test]
fn url_plugins_are_cached_and_survive_a_lost_connection() {
    let l = MapLoader::default();
    l.set("http://a.com/p.wasm", &module_of("one"));
    let base = u("http://a.com/page");
    let c = no_compiler();
    let mut r = Registry::in_memory();
    let id = r.install(key("a.com", ""), Plugin::Url("/p.wasm".into()), &base, &l).unwrap();
    // The stored URL is absolute, and a module that was reachable is kept already.
    assert_eq!(r.get(id).unwrap().plugin, Plugin::Url("http://a.com/p.wasm".into()));
    l.drop_all();
    assert_eq!(r.modules_for(&base, &l, &c)[0].1.as_ref().unwrap(), &module_of("one"));
    // New content is picked up and cached when the network returns.
    l.set("http://a.com/p.wasm", &module_of("two"));
    assert_eq!(r.modules_for(&base, &l, &c)[0].1.as_ref().unwrap(), &module_of("two"));
    l.drop_all();
    assert_eq!(r.modules_for(&base, &l, &c)[0].1.as_ref().unwrap(), &module_of("two"));
}

#[test]
fn url_plugin_installed_offline_fails_only_until_fetched() {
    let l = MapLoader::default();
    let base = u("http://a.com/");
    let c = no_compiler();
    let mut r = Registry::in_memory();
    r.install(key("a.com", ""), Plugin::Url("/p.wasm".into()), &base, &l).unwrap();
    assert!(matches!(r.modules_for(&base, &l, &c)[0].1, Err(PluginError::Fetch(_))));
    l.set("http://a.com/p.wasm", &module_of("ok"));
    assert!(r.modules_for(&base, &l, &c)[0].1.is_ok());
}

#[cfg(unix)]
#[test]
fn url_plugin_source_is_compiled_and_its_module_cached_for_offline_use() {
    let (c, dir) = fake_compiler("urlsrc");
    let l = MapLoader::default();
    l.set("http://a.com/p.rs", b"remote source");
    let base = u("http://a.com/");
    let mut r = Registry::in_memory();
    let id = r.install(key("a.com", ""), Plugin::Url("/p.rs".into()), &base, &l).unwrap();
    assert!(r.get(id).unwrap().cache.is_none(), "source is not compiled at install");
    assert_eq!(r.modules_for(&base, &l, &c)[0].1.as_ref().unwrap(), &module_of("remote source"));
    l.drop_all();
    assert_eq!(r.modules_for(&base, &l, &c)[0].1.as_ref().unwrap(), &module_of("remote source"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plugins_are_saved_and_reloaded_in_order() {
    let dir = tmpdir("persist");
    let l = MapLoader::default();
    l.set("http://a.com/p.wasm", &module_of("remote"));
    let base = u("http://a.com/");
    let k = key("a.com", "app");
    let c = no_compiler();
    let (a, b, cc, d);
    {
        let mut r = Registry::open(&dir).unwrap();
        a = r.install(k.clone(), Plugin::Source("fn a() {}\n\nfn z() {}\n".into()), &base, &l).unwrap();
        b = r.install(k.clone(), Plugin::Wasm(module_of("b")), &base, &l).unwrap();
        cc = r.install(k.clone(), Plugin::Url("/p.wasm".into()), &base, &l).unwrap();
        d = r.install(key("a.com", "gone"), Plugin::Source("d".into()), &base, &l).unwrap();
        r.delete(d).unwrap();
    }
    l.drop_all();
    let mut r = Registry::open(&dir).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(r.len(), 3);
    assert!(r.get(d).is_none());
    // Source with blank lines survives byte for byte.
    assert_eq!(r.get(a).unwrap().plugin, Plugin::Source("fn a() {}\n\nfn z() {}\n".into()));
    assert_eq!(r.get(b).unwrap().plugin, Plugin::Wasm(module_of("b")));
    // Offline reload still gives all of them in order, the URL one from its cache.
    let mods = r.modules_for(&u("http://a.com/app/x"), &l, &c);
    assert_eq!(mods.iter().map(|p| p.0).collect::<Vec<_>>(), vec![a, b, cc]);
    assert!(mods[0].1.is_err(), "no compiler here, so the source one fails");
    assert!(mods[1].1.is_ok() && mods[2].1.is_ok());
    // New installs continue the order.
    let e = r.install(k.clone(), Plugin::Source("e".into()), &base, &l).unwrap();
    let ids: Vec<_> = r.modules_for(&u("http://a.com/app"), &l, &c).into_iter().map(|p| p.0).collect();
    assert_eq!(ids, vec![a, b, cc, e]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn update_and_clear_change_the_files() {
    let dir = tmpdir("files");
    let l = MapLoader::default();
    let base = u("http://a.com/");
    let k = key("a.com", "");
    let mut r = Registry::open(&dir).unwrap();
    let a = r.install(k.clone(), Plugin::Source("a".into()), &base, &l).unwrap();
    r.update(a, Plugin::Source("a2".into()), &base, &l).unwrap();
    drop(r);
    let r = Registry::open(&dir).unwrap();
    assert_eq!(r.get(a).unwrap().plugin, Plugin::Source("a2".into()));
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
