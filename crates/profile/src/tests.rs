use super::*;

use std::path::PathBuf;

use plugin::{AppKey, Plugin};

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "brewster-profile-test-{}-{}",
        std::process::id(),
        name
    ));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn paths(root: &Path) -> Paths {
    Paths {
        home: Some(root.join("home")),
        tmp: root.join("tmp").join("plugins"),
        config: Some(root.join("home").join(".config").join("brewster")),
    }
}

#[test]
fn names_are_checked() {
    for bad in ["", "a/b", "..", "a b", "é", &"x".repeat(65), "a.b"] {
        assert!(check_name(bad).is_err(), "{:?}", bad);
        assert!(Profile::new(bad).is_err());
    }
    assert!(check_name("work_2-b").is_ok());
}

#[test]
fn paths_are_per_profile_and_sandboxed() {
    let root = tmp("paths");
    let p = paths(&root);
    let a = p.plugin_dir("a", Storage::Home).unwrap();
    let b = p.plugin_dir("b", Storage::Home).unwrap();
    assert_ne!(a, b);
    assert!(a.starts_with(root.join("home").join(".brewster").join("plugins")));
    assert!(
        p.plugin_dir("a", Storage::Tmp)
            .unwrap()
            .starts_with(root.join("tmp"))
    );
    assert!(p.plugin_dir("../x", Storage::Home).is_err());
    assert!(p.profile_file("a").unwrap().ends_with("profiles/a.toml"));
    // Without a home directory plugins fall back to the temporary place.
    let nohome = Paths {
        home: None,
        tmp: root.join("t"),
        config: None,
    };
    assert_eq!(
        nohome.plugin_dir("a", Storage::Home).unwrap(),
        root.join("t").join("a")
    );
    assert!(nohome.profile_file("a").is_err());
}

fn full() -> Profile {
    let mut p = Profile::new("work").unwrap();
    p.storage = Storage::Tmp;
    p.settings
        .set("tabs.position", settings::Value::str("left"))
        .unwrap();
    p.settings
        .set(
            "keybinds.quit",
            settings::Value::List(vec![settings::Value::str("ctrl-q")]),
        )
        .unwrap();
    p.tabs = vec![
        SavedTab {
            url: "file:///a.rs".into(),
            title: "A \"quoted\" title".into(),
        },
        SavedTab {
            url: "https://x.y/z?q=1#f".into(),
            title: String::new(),
        },
    ];
    p.active_tab = 1;
    p.history = vec![HistoryEntry {
        url: "about:blank".into(),
        title: "t".into(),
        visited: 1700000000,
    }];
    p.applications = vec![AppRecord {
        app: "x.y/shop".into(),
        plugins: vec!["id1".into(), "id2".into()],
    }];
    p
}

#[test]
fn every_format_round_trips_a_whole_profile() {
    let root = tmp("rt");
    for ext in ["toml", "json", "yaml", "yml"] {
        let path = root.join(format!("p.{}", ext));
        let mut p = full();
        p.save_as(&path).unwrap();
        let (q, warnings) = Profile::load(&path).unwrap();
        assert!(warnings.is_empty(), "{}: {:?}", ext, warnings);
        assert_eq!(q.name, "work");
        assert_eq!(q.storage, Storage::Tmp);
        assert_eq!(q.settings, p.settings, "{}", ext);
        assert_eq!(q.tabs, p.tabs);
        assert_eq!(q.active_tab, 1);
        assert_eq!(q.history, p.history);
        assert_eq!(q.applications, p.applications);
        assert_eq!(q.path.as_deref(), Some(path.as_path()));
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn only_changed_settings_are_written() {
    let root = tmp("changed");
    let path = root.join("p.toml");
    let mut p = full();
    p.save_as(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("position"));
    assert!(!text.contains("sidebar_width"), "{}", text);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unknown_keys_survive_a_save() {
    let root = tmp("extra");
    let path = root.join("p.json");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        &path,
        r#"{"name": "w", "future": {"x": [1, 2]}, "settings": {"mystery": 5}}"#,
    )
    .unwrap();
    let (p, w) = Profile::load(&path).unwrap();
    assert!(w.is_empty());
    p.save().unwrap();
    let v = settings::read_file(&path).unwrap();
    assert_eq!(v.get("future.x").unwrap().as_list().unwrap().len(), 2);
    assert_eq!(v.get("settings.mystery"), Some(&settings::Value::Int(5)));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn bad_values_warn_and_good_ones_load() {
    let root = tmp("warn");
    let path = root.join("p.yaml");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        &path,
        "storage: nowhere\nsettings:\n  tabs:\n    position: sideways\n    sidebar_width: 30\ntabs:\n  - url: a.rs\n  - title: no url\nactive_tab: 9\n",
    )
    .unwrap();
    let (p, w) = Profile::load(&path).unwrap();
    assert_eq!(w.len(), 3, "{:?}", w);
    assert_eq!(p.storage, Storage::Home);
    assert_eq!(p.settings.int("tabs.sidebar_width"), 30);
    assert_eq!(p.settings.str("tabs.position"), "top");
    assert_eq!(p.tabs.len(), 1);
    assert_eq!(p.active_tab, 0);
    assert_eq!(p.name, "p");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn newer_or_malformed_files_are_errors() {
    let root = tmp("err");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("p.toml");
    std::fs::write(&path, "version = 99\n").unwrap();
    assert!(Profile::load(&path).unwrap_err().contains("newer"));
    std::fs::write(&path, "name = \"../evil\"\n").unwrap();
    assert!(Profile::load(&path).is_err());
    std::fs::write(&path, "this is not toml").unwrap();
    assert!(Profile::load(&path).is_err());
    assert!(Profile::load(&root.join("missing.toml")).is_err());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn load_or_new_starts_fresh_and_saves_where_asked() {
    let root = tmp("new");
    let path = root.join("sub").join("me.toml");
    let (p, w) = Profile::load_or_new(&path, "me").unwrap();
    assert!(w.is_empty());
    assert_eq!(p.name, "me");
    assert!(!path.exists());
    p.save().unwrap();
    assert!(path.exists());
    assert_eq!(Profile::load(&path).unwrap().0.name, "me");
    assert!(Profile::new("x").unwrap().save().is_err());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn plugins_live_in_the_profile_sandbox_and_come_back() {
    let root = tmp("plugins");
    let ps = paths(&root);
    let mut a = Profile::new("a").unwrap();
    a.storage = Storage::Tmp;
    let mut reg = a.open_registry(&ps).unwrap();
    let base = location::Url::parse("https://x.y/shop/").unwrap();
    let loader = location::DefaultLoader::default();
    let key = AppKey::new("x.y", "shop").unwrap();
    let id = reg
        .install(key, Plugin::Source("fn page() {}".into()), &base, &loader)
        .unwrap();
    a.sync_applications(&reg);
    assert_eq!(
        a.applications,
        vec![AppRecord {
            app: "x.y/shop".into(),
            plugins: vec![id.to_string()]
        }]
    );
    assert!(root.join("tmp").join("plugins").join("a").exists());

    // The same profile reopens the plugin; another profile does not see it.
    assert_eq!(a.open_registry(&ps).unwrap().len(), 1);
    let mut b = Profile::new("b").unwrap();
    b.storage = Storage::Tmp;
    assert_eq!(b.open_registry(&ps).unwrap().len(), 0);
    let _ = std::fs::remove_dir_all(&root);
}
