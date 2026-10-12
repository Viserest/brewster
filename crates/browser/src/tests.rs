use super::*;

use std::collections::HashMap;
use std::path::PathBuf;

use engine::Registry;
use profile::Profile;
use settings::Value;

struct Mock(HashMap<String, String>);

impl Loader for Mock {
    fn load(&self, url: &Url) -> Result<String, String> {
        self.0
            .get(&url.to_string())
            .cloned()
            .ok_or_else(|| format!("{} not found", url))
    }
}

const A: &str = "h1 \"Page A\"\nlink \"/b.cre\" \"to b\"\nlink https://far.away/x \"far\"\ninput \"name\" \"\"\nbutton.onclick:hit \"press\"\nfn hit(self: Element) { self.set_text(\"pressed\") }\n";
const B: &str = "h1 \"Page B\"\np \"hello from b\"\n";

fn files() -> Mock {
    let mut m = HashMap::new();
    m.insert("file:///a.cre".to_string(), A.to_string());
    m.insert("file:///b.cre".to_string(), B.to_string());
    m.insert("file:///c.cre".to_string(), "h1 \"Page C\"\n".to_string());
    Mock(m)
}

fn browser_with(profile: Profile, urls: &[&str]) -> Browser {
    let mut b = Browser::new(profile, Registry::in_memory(), Rc::new(files()), 60, 20);
    let urls: Vec<String> = urls.iter().map(|s| s.to_string()).collect();
    b.start(&urls);
    b.take_effects();
    b
}

fn mk(urls: &[&str]) -> Browser {
    browser_with(Profile::new("test").unwrap(), urls)
}

fn key(k: Key) -> KeyPress {
    KeyPress::plain(k)
}
fn ch(c: char) -> KeyPress {
    key(Key::Char(c))
}
fn ctrl(c: char) -> KeyPress {
    KeyPress::ctrl(Key::Char(c))
}
fn with(k: Key, ctrl: bool, alt: bool, shift: bool) -> KeyPress {
    KeyPress::new(k, Mods { ctrl, alt, shift })
}
fn type_text(b: &mut Browser, s: &str) {
    for c in s.chars() {
        b.on_key(ch(c));
    }
}
fn text(b: &Browser) -> Vec<String> {
    let s = b.screen();
    (0..s.rows).map(|y| s.row_text(y)).collect()
}
fn contains(b: &Browser, needle: &str) -> bool {
    text(b).iter().any(|l| l.contains(needle))
}
fn row_with(b: &Browser, needle: &str) -> Option<usize> {
    text(b).iter().position(|l| l.contains(needle))
}
fn click(col: usize, row: usize, button: Button, mods: Mods) -> MouseEvent {
    MouseEvent {
        col,
        row,
        kind: MouseKind::Down(button),
        mods,
    }
}
fn left(col: usize, row: usize) -> MouseEvent {
    click(col, row, Button::Left, Mods::default())
}
fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "brewster-browser-test-{}-{}",
        std::process::id(),
        name
    ));
    let _ = std::fs::remove_dir_all(&p);
    p
}

// ── tabs ────────────────────────────────────────────────────────────

#[test]
fn starts_on_the_home_page() {
    let b = mk(&[]);
    assert_eq!(b.tab_count(), 1);
    assert_eq!(b.tab_url(0).as_deref(), Some("about:newtab"));
    assert!(contains(&b, "brewster"));
    assert!(contains(&b, "Open settings"));
}

#[test]
fn opens_the_given_addresses_as_tabs() {
    let b = mk(&["/a.cre", "/b.cre"]);
    assert_eq!(b.tab_count(), 2);
    assert_eq!(b.active_tab(), 0);
    assert_eq!(b.tab_title(0), Some("Page A"));
    assert_eq!(b.tab_title(1), Some("Page B"));
}

#[test]
fn the_tab_bar_is_on_top_by_default_and_the_status_line_is_last() {
    let b = mk(&["/a.cre", "/b.cre"]);
    let t = text(&b);
    assert!(
        t[0].contains("Page A") && t[0].contains("Page B") && t[0].contains('×'),
        "{:?}",
        t[0]
    );
    assert!(t[1].starts_with("PAGE A"), "{:?}", t[1]);
    assert!(t[19].contains("file:///a.cre"), "{:?}", t[19]);
    let s = b.screen();
    assert!(
        s.get(1, 0).unwrap().reverse,
        "the active tab is highlighted"
    );
    assert!(!s.get(0, 19).is_none() && s.get(0, 19).unwrap().reverse);
}

#[test]
fn tabs_show_multiple_hides_a_lone_tab_bar() {
    let mut p = Profile::new("t").unwrap();
    p.settings.set("tabs.show", Value::str("multiple")).unwrap();
    let mut b = browser_with(p, &["/a.cre"]);
    assert!(text(&b)[0].starts_with("PAGE A"));
    b.on_key(ctrl('t'));
    assert!(text(&b)[0].contains("×"));
    let mut p = Profile::new("t").unwrap();
    p.settings.set("tabs.show", Value::str("never")).unwrap();
    let b = browser_with(p, &["/a.cre", "/b.cre"]);
    assert!(text(&b)[0].starts_with("PAGE A"));
}

#[test]
fn tab_bar_position_bottom() {
    let mut p = Profile::new("t").unwrap();
    p.settings
        .set("tabs.position", Value::str("bottom"))
        .unwrap();
    let b = mk_with_profile(p);
    let t = text(&b);
    assert!(t[0].starts_with("PAGE A"));
    assert!(
        t[18].contains("Page A") && t[18].contains("Page B"),
        "{:?}",
        t[18]
    );
    assert!(t[19].contains("file:///a.cre"));
}

fn mk_with_profile(p: Profile) -> Browser {
    browser_with(p, &["/a.cre", "/b.cre"])
}

#[test]
fn tab_bar_position_left_and_right() {
    let mut p = Profile::new("t").unwrap();
    p.settings.set("tabs.position", Value::str("left")).unwrap();
    p.settings
        .set("tabs.sidebar_width", Value::Int(16))
        .unwrap();
    let b = mk_with_profile(p);
    let t = text(&b);
    assert!(
        t[0].starts_with(" Page A") && t[0].chars().nth(15) == Some('│'),
        "{:?}",
        t[0]
    );
    assert!(t[1].starts_with(" Page B"));
    assert!(t[2].starts_with(" + new tab"));
    assert!(
        t[0].chars()
            .skip(16)
            .collect::<String>()
            .starts_with("PAGE A"),
        "{:?}",
        t[0]
    );
    assert_eq!(b.viewer().width, 44);

    let mut p = Profile::new("t").unwrap();
    p.settings
        .set("tabs.position", Value::str("right"))
        .unwrap();
    p.settings
        .set("tabs.sidebar_width", Value::Int(16))
        .unwrap();
    let b = mk_with_profile(p);
    let t = text(&b);
    assert!(t[0].starts_with("PAGE A"), "{:?}", t[0]);
    assert_eq!(t[0].chars().nth(44), Some('│'));
    assert!(t[0].chars().skip(45).collect::<String>().contains("Page A"));
    assert_eq!(b.viewer().width, 44);
}

#[test]
fn keys_open_close_and_switch_tabs() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ctrl('t'));
    assert_eq!(b.tab_count(), 2);
    assert_eq!(b.active_tab(), 1);
    assert_eq!(b.tab_url(1).as_deref(), Some("about:newtab"));
    b.on_key(with(Key::Left, false, true, false));
    assert_eq!(b.active_tab(), 0);
    b.on_key(with(Key::Right, false, true, false));
    assert_eq!(b.active_tab(), 1);
    b.on_key(with(Key::Right, false, true, false));
    assert_eq!(b.active_tab(), 0, "wraps around");
    b.on_key(ctrl('w'));
    assert_eq!(b.tab_count(), 1);
    assert_eq!(b.tab_url(0).as_deref(), Some("about:newtab"));
    assert!(!b.should_quit());
    b.on_key(ctrl('w'));
    assert!(b.should_quit(), "closing the last tab quits");
    assert!(b.take_effects().contains(&Effect::Quit));
}

#[test]
fn closing_a_tab_before_the_active_one_keeps_the_active_page() {
    let mut b = mk(&["/a.cre", "/b.cre", "/c.cre"]);
    b.on_key(with(Key::Right, false, true, false));
    b.on_key(with(Key::Right, false, true, false));
    assert_eq!(b.tab_title(b.active_tab()), Some("Page C"));
    b.on_mouse(click(1, 0, Button::Middle, Mods::default()));
    assert_eq!(b.tab_count(), 2);
    assert_eq!(b.tab_title(b.active_tab()), Some("Page C"));
}

#[test]
fn clicking_the_tab_bar() {
    let mut b = mk(&["/a.cre", "/b.cre"]);
    let t0 = text(&b)[0].clone();
    let b_col = t0.find("Page B").unwrap();
    b.on_mouse(left(b_col, 0));
    assert_eq!(b.active_tab(), 1);
    // The × of the first tab closes it.
    let x_col = t0.chars().position(|c| c == '×').unwrap();
    b.on_mouse(left(x_col, 0));
    assert_eq!(b.tab_count(), 1);
    assert_eq!(b.tab_title(0), Some("Page B"));
    // The + button opens a new tab.
    let t0 = text(&b)[0].clone();
    let plus = t0.chars().position(|c| c == '+').unwrap();
    b.on_mouse(left(plus, 0));
    assert_eq!(b.tab_count(), 2);
    assert_eq!(b.tab_url(1).as_deref(), Some("about:newtab"));
}

#[test]
fn many_tabs_scroll_the_bar_to_the_active_one() {
    let mut b = mk(&["/a.cre"]);
    for _ in 0..12 {
        b.on_key(ctrl('t'));
    }
    assert_eq!(b.active_tab(), 12);
    let t0 = text(&b)[0].clone();
    assert!(t0.chars().count() == 60);
    let s = b.screen();
    // The active tab (the last) is drawn reversed somewhere on the bar.
    assert!(
        (0..60).any(|x| s.get(x, 0).unwrap().bold && s.get(x, 0).unwrap().reverse),
        "{:?}",
        t0
    );
}

// ── links ───────────────────────────────────────────────────────────

#[test]
fn enter_follows_a_link_in_the_same_tab() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(key(Key::Enter));
    assert_eq!(b.tab_count(), 1);
    assert_eq!(b.tab_url(0).as_deref(), Some("file:///b.cre"));
    assert!(contains(&b, "hello from b"));
    assert_eq!(b.tab_title(0), Some("Page B"));
}

#[test]
fn ctrl_enter_and_t_open_a_new_tab() {
    for k in [with(Key::Enter, true, false, false), ch('t')] {
        let mut b = mk(&["/a.cre"]);
        b.on_key(k);
        assert_eq!(b.tab_count(), 2);
        assert_eq!(b.active_tab(), 1);
        assert_eq!(b.tab_url(0).as_deref(), Some("file:///a.cre"));
        assert_eq!(b.tab_url(1).as_deref(), Some("file:///b.cre"));
    }
}

#[test]
fn shift_enter_and_w_open_a_window_when_there_are_windows() {
    let mut b = mk(&["/a.cre"]);
    b.set_capabilities(Capabilities {
        windows: true,
        scaling: false,
    });
    b.on_key(with(Key::Enter, false, false, true));
    assert_eq!(
        b.take_effects(),
        vec![Effect::NewWindow("file:///b.cre".to_string())]
    );
    assert_eq!(b.tab_count(), 1);
    b.on_key(ch('w'));
    assert_eq!(
        b.take_effects(),
        vec![Effect::NewWindow("file:///b.cre".to_string())]
    );
}

#[test]
fn without_windows_a_new_window_is_a_tab_and_says_so() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ch('w'));
    assert_eq!(b.tab_count(), 2);
    assert!(b.take_effects().is_empty());
    assert!(b.status().contains("one window"), "{}", b.status());
}

#[test]
fn links_are_resolved_against_the_page() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ch('j')); // the far link
    b.on_key(key(Key::Enter));
    // The mock has no such page, so an error page is shown, and the tab stays a tab.
    assert_eq!(b.tab_url(0).as_deref(), Some("https://far.away/x"));
    assert!(contains(&b, "CANNOT OPEN THIS PAGE"));
    assert!(contains(&b, "not found"));
    assert!(b.status().contains("not found"));
    // The error page links home.
    b.on_key(key(Key::Enter));
    assert_eq!(b.tab_url(0).as_deref(), Some("about:newtab"));
}

#[test]
fn clicking_links_with_the_mouse() {
    let find = |b: &Browser| row_with(b, "to b").unwrap();
    // plain click: same tab
    let mut b = mk(&["/a.cre"]);
    let y = find(&b);
    b.on_mouse(left(1, y));
    assert_eq!(b.tab_url(0).as_deref(), Some("file:///b.cre"));
    // ctrl-click: new tab
    let mut b = mk(&["/a.cre"]);
    let y = find(&b);
    b.on_mouse(click(
        1,
        y,
        Button::Left,
        Mods {
            ctrl: true,
            alt: false,
            shift: false,
        },
    ));
    assert_eq!(b.tab_count(), 2);
    assert_eq!(b.tab_url(1).as_deref(), Some("file:///b.cre"));
    // middle-click: new tab
    let mut b = mk(&["/a.cre"]);
    let y = find(&b);
    b.on_mouse(click(1, y, Button::Middle, Mods::default()));
    assert_eq!(b.tab_count(), 2);
    // shift-click: new window
    let mut b = mk(&["/a.cre"]);
    b.set_capabilities(Capabilities {
        windows: true,
        scaling: false,
    });
    let y = find(&b);
    b.on_mouse(click(
        1,
        y,
        Button::Left,
        Mods {
            ctrl: false,
            alt: false,
            shift: true,
        },
    ));
    assert_eq!(
        b.take_effects(),
        vec![Effect::NewWindow("file:///b.cre".to_string())]
    );
    // an unbound chord only selects
    let mut b = mk(&["/a.cre"]);
    let y = find(&b);
    b.on_mouse(click(
        1,
        y,
        Button::Left,
        Mods {
            ctrl: false,
            alt: true,
            shift: false,
        },
    ));
    assert_eq!(b.tab_count(), 1);
    assert_eq!(b.tab_url(0).as_deref(), Some("file:///a.cre"));
}

#[test]
fn clicking_an_input_edits_it_and_a_button_runs_its_function() {
    let mut b = mk(&["/a.cre"]);
    let y = row_with(&b, "name:").unwrap();
    b.on_mouse(left(1, y));
    assert!(b.viewer().editing().is_some());
    type_text(&mut b, "q");
    assert!(
        !b.should_quit(),
        "q is typed into the input, not a quit key"
    );
    assert!(contains(&b, "name: [q]"));
    b.on_key(key(Key::Esc));
    let y = row_with(&b, "press").unwrap();
    b.on_mouse(left(1, y));
    assert!(contains(&b, "pressed"));
}

#[test]
fn mouse_wheel_scrolls_the_page() {
    let mut src = String::new();
    for i in 0..60 {
        src.push_str(&format!("p \"line {}\"\n", i));
    }
    let mut m = files();
    m.0.insert("file:///long.cre".to_string(), src);
    let mut b = Browser::new(
        Profile::new("t").unwrap(),
        Registry::in_memory(),
        Rc::new(m),
        40,
        12,
    );
    b.start(&["/long.cre".to_string()]);
    b.on_mouse(MouseEvent {
        col: 5,
        row: 5,
        kind: MouseKind::Wheel(1),
        mods: Mods::default(),
    });
    assert_eq!(b.viewer().top, 3);
    b.on_mouse(MouseEvent {
        col: 5,
        row: 5,
        kind: MouseKind::Wheel(-5),
        mods: Mods::default(),
    });
    assert_eq!(b.viewer().top, 0);
}

// ── key bindings ────────────────────────────────────────────────────

#[test]
fn quit_keys_and_hard_ctrl_c() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ch('q'));
    assert!(b.should_quit());
    let mut b = mk(&["/a.cre"]);
    b.on_key(ctrl('q'));
    assert!(b.should_quit());
    let mut b = mk(&["/a.cre"]);
    b.on_key(ctrl('c'));
    assert!(b.should_quit());
}

#[test]
fn typing_in_an_input_does_not_run_bindings_but_ctrl_chords_still_work() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ch('j'));
    b.on_key(ch('j'));
    b.on_key(key(Key::Enter)); // edit the input
    assert!(b.viewer().editing().is_some());
    type_text(&mut b, "tqrw");
    assert!(contains(&b, "name: [tqrw]"));
    assert_eq!(b.tab_count(), 1);
    b.on_key(ctrl('t'));
    assert_eq!(b.tab_count(), 2, "ctrl chords work while typing");
}

#[test]
fn rebinding_keys_through_settings() {
    let mut p = Profile::new("t").unwrap();
    p.settings
        .set(
            "keybinds.quit",
            Value::List(vec![Value::str("x"), Value::str("ctrl-F4")]),
        )
        .unwrap();
    p.settings
        .set("keybinds.link_open", Value::List(vec![Value::str("space")]))
        .unwrap();
    let mut b = browser_with(p, &["/a.cre"]);
    b.on_key(ch('q'));
    assert!(!b.should_quit(), "q is no longer bound");
    b.on_key(key(Key::Enter));
    assert_eq!(
        b.tab_url(0).as_deref(),
        Some("file:///a.cre"),
        "enter no longer follows links"
    );
    b.on_key(ch(' '));
    assert_eq!(b.tab_url(0).as_deref(), Some("file:///b.cre"));
    b.on_key(ch('x'));
    assert!(b.should_quit());
}

#[test]
fn bad_bindings_are_reported_and_skipped() {
    let mut p = Profile::new("t").unwrap();
    p.settings
        .set(
            "keybinds.quit",
            Value::List(vec![Value::str("ctrl-ctrl-q"), Value::str("z")]),
        )
        .unwrap();
    let mut b = browser_with(p, &["/a.cre"]);
    assert!(b.status().contains("keybinds.quit"), "{}", b.status());
    b.on_key(ch('z'));
    assert!(b.should_quit());
}

#[test]
fn binding_parser() {
    use Binding::Click;
    let k = |c: Key, ctrl: bool, alt: bool, shift: bool| {
        Binding::Key(KeyPress::new(c, Mods { ctrl, alt, shift }))
    };
    assert_eq!(
        parse_binding("ctrl-t").unwrap(),
        k(Key::Char('t'), true, false, false)
    );
    assert_eq!(
        parse_binding("CTRL-T").unwrap(),
        k(Key::Char('t'), true, false, false)
    );
    assert_eq!(
        parse_binding("alt-right").unwrap(),
        k(Key::Right, false, true, false)
    );
    assert_eq!(
        parse_binding("shift-enter").unwrap(),
        k(Key::Enter, false, false, true)
    );
    assert_eq!(
        parse_binding("f5").unwrap(),
        k(Key::F(5), false, false, false)
    );
    assert_eq!(
        parse_binding("-").unwrap(),
        k(Key::Char('-'), false, false, false)
    );
    assert_eq!(
        parse_binding("ctrl--").unwrap(),
        k(Key::Char('-'), true, false, false)
    );
    assert_eq!(
        parse_binding("T").unwrap(),
        k(Key::Char('T'), false, false, false)
    );
    assert_eq!(
        parse_binding("ctrl-click").unwrap(),
        Click {
            button: Button::Left,
            mods: Mods {
                ctrl: true,
                alt: false,
                shift: false
            }
        }
    );
    assert!(matches!(
        parse_binding("middle-click").unwrap(),
        Click {
            button: Button::Middle,
            ..
        }
    ));
    for bad in ["", "ctrl-", "f13", "f0", "ctrl-ctrl-a", "foo", "ctrl-alt"] {
        assert!(parse_binding(bad).is_err(), "{:?}", bad);
    }
}

#[test]
fn uppercase_letters_match_whatever_the_shift_report() {
    let mut p = Profile::new("t").unwrap();
    p.settings
        .set("keybinds.reload", Value::List(vec![Value::str("R")]))
        .unwrap();
    let mut b = browser_with(p, &["/a.cre"]);
    b.on_key(with(Key::Char('R'), false, false, true));
    assert!(b.status().contains("reloaded"));
    b.on_key(with(Key::Char('R'), false, false, false));
    assert!(b.status().contains("reloaded"));
}

#[test]
fn reload_drops_edits_and_picks_up_the_new_source() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ch('j'));
    b.on_key(ch('j'));
    b.on_key(key(Key::Enter));
    type_text(&mut b, "zz");
    b.on_key(key(Key::Esc));
    assert!(contains(&b, "name: [zz]"));
    b.on_key(ch('r'));
    assert!(contains(&b, "name: []"));
    assert!(b.status().contains("reloaded"));
}

#[test]
fn open_prompt_navigates_the_current_tab() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ctrl('l'));
    assert!(b.overlay_open());
    assert!(contains(&b, "address:"));
    // The current address is prefilled; clear it and type another.
    b.on_key(ctrl('u'));
    type_text(&mut b, "/c.cre");
    b.on_key(key(Key::Enter));
    assert!(!b.overlay_open());
    assert_eq!(b.tab_url(0).as_deref(), Some("file:///c.cre"));
    assert_eq!(b.tab_title(0), Some("Page C"));
    // Esc cancels
    b.on_key(ctrl('l'));
    b.on_key(key(Key::Esc));
    assert_eq!(b.tab_url(0).as_deref(), Some("file:///c.cre"));
    // paste goes into the prompt
    b.on_key(ctrl('l'));
    b.on_key(ctrl('u'));
    b.on_paste("/b.cre".to_string());
    b.on_key(key(Key::Enter));
    assert_eq!(b.tab_url(0).as_deref(), Some("file:///b.cre"));
}

// ── settings box ────────────────────────────────────────────────────

#[test]
fn ctrl_p_opens_the_settings_box_in_the_middle_with_search_on_top() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ctrl('p'));
    assert!(b.settings_open());
    let t = text(&b);
    let top = t.iter().position(|l| l.contains("┌")).unwrap();
    let bottom = t.iter().rposition(|l| l.contains("└")).unwrap();
    assert!(top > 0 && bottom < 19);
    assert!(
        (top + bottom) as isize - 19 >= -2 && (top + bottom) as isize - 19 <= 2,
        "centered: {} {}",
        top,
        bottom
    );
    assert!(t[top].contains("settings"));
    assert!(t[top + 1].contains("search:"));
    assert!(t[top + 3].contains("general.homepage"), "{:?}", t[top + 3]);
    // Esc closes, ctrl-p toggles
    b.on_key(key(Key::Esc));
    assert!(!b.settings_open());
    b.on_key(ctrl('p'));
    b.on_key(ctrl('p'));
    assert!(!b.settings_open());
    b.on_key(key(Key::F(1)));
    assert!(b.settings_open());
}

#[test]
fn searching_filters_the_list() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ctrl('p'));
    type_text(&mut b, "sidebar");
    let t = text(&b);
    assert!(t.iter().any(|l| l.contains("tabs.sidebar_width")));
    assert!(!t.iter().any(|l| l.contains("keybinds.quit")));
    assert!(!b.should_quit(), "typing q... goes to the search, not quit");
    b.on_key(ctrl('u'));
    type_text(&mut b, "zzzz");
    assert!(contains(&b, "no matching options"));
    b.on_key(ctrl('u'));
    type_text(&mut b, "quit");
    assert!(contains(&b, "keybinds.quit"));
    assert!(!b.should_quit());
}

#[test]
fn enter_cycles_a_choice_and_the_change_applies_at_once() {
    let mut b = mk(&["/a.cre", "/b.cre"]);
    b.on_key(ctrl('p'));
    type_text(&mut b, "tabs.position");
    b.on_key(key(Key::Enter)); // top -> bottom
    assert_eq!(b.settings().str("tabs.position"), "bottom");
    assert!(b.settings_unsaved());
    b.on_key(key(Key::Esc));
    let t = text(&b);
    assert!(t[0].starts_with("PAGE A"), "{:?}", t[0]);
    assert!(
        t[18].contains("Page A") && t[18].contains("Page B"),
        "{:?}",
        t[18]
    );
    assert!(b.status().contains("not saved"));
    // Keep cycling: bottom -> left -> right -> top
    b.on_key(ctrl('p'));
    type_text(&mut b, "tabs.position");
    for expect in ["left", "right", "top"] {
        b.on_key(key(Key::Enter));
        assert_eq!(b.settings().str("tabs.position"), expect);
    }
}

#[test]
fn enter_toggles_a_boolean_and_cursor_mode_applies_to_open_tabs() {
    let mut b = mk(&["/a.cre"]);
    assert_eq!(b.viewer().cursor.mode(), Mode::Focused);
    b.on_key(ctrl('p'));
    type_text(&mut b, "cursor_mode");
    b.on_key(key(Key::Enter));
    assert_eq!(b.settings().str("view.cursor_mode"), "all");
    assert_eq!(b.viewer().cursor.mode(), Mode::All);
    b.on_key(ctrl('u'));
    type_text(&mut b, "fill_window");
    b.on_key(key(Key::Enter));
    assert!(!b.settings().bool("view.fill_window"));
}

#[test]
fn numbers_and_lists_are_edited_as_text_and_checked() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ctrl('p'));
    type_text(&mut b, "sidebar_width");
    b.on_key(key(Key::Enter));
    assert!(contains(&b, "tabs.sidebar_width: 24"), "{:?}", text(&b));
    b.on_key(ctrl('u'));
    type_text(&mut b, "3");
    b.on_key(key(Key::Enter));
    assert_eq!(
        b.settings().int("tabs.sidebar_width"),
        24,
        "3 is out of range"
    );
    assert!(b.settings_open());
    assert!(contains(&b, "between"));
    // The box is still editing; fix the value.
    // (the failed commit ended editing, so start again)
    b.on_key(key(Key::Enter));
    b.on_key(ctrl('u'));
    type_text(&mut b, "30");
    b.on_key(key(Key::Enter));
    assert_eq!(b.settings().int("tabs.sidebar_width"), 30);

    // lists: comma separated
    b.on_key(ctrl('u'));
    type_text(&mut b, "keybinds.close");
    b.on_key(key(Key::Enter));
    b.on_key(ctrl('u'));
    type_text(&mut b, "ctrl-w, ctrl-x");
    b.on_key(key(Key::Enter));
    assert_eq!(
        b.settings().list("keybinds.close_tab"),
        vec!["ctrl-w", "ctrl-x"]
    );
    b.on_key(key(Key::Esc));
    b.on_key(ctrl('t'));
    b.on_key(ctrl('x'));
    assert_eq!(b.tab_count(), 1, "the new binding works");
}

#[test]
fn esc_cancels_an_edit_and_ctrl_r_resets() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ctrl('p'));
    type_text(&mut b, "title_width");
    b.on_key(key(Key::Enter));
    b.on_key(ctrl('u'));
    type_text(&mut b, "30");
    b.on_key(key(Key::Esc));
    assert!(b.settings_open(), "esc only cancels the edit");
    assert_eq!(b.settings().int("tabs.title_width"), 18);
    b.on_key(key(Key::Enter));
    b.on_key(ctrl('u'));
    type_text(&mut b, "30");
    b.on_key(key(Key::Enter));
    assert_eq!(b.settings().int("tabs.title_width"), 30);
    b.on_key(ctrl('r'));
    assert_eq!(b.settings().int("tabs.title_width"), 18);
    assert!(!b.settings_unsaved());
}

#[test]
fn arrows_move_the_selection_and_the_list_scrolls() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ctrl('p'));
    for _ in 0..30 {
        b.on_key(key(Key::Down));
    }
    let t = text(&b);
    let sel: Vec<&String> = t.iter().filter(|l| l.contains("│> ")).collect();
    assert_eq!(sel.len(), 1, "{:#?}", t);
    // The last option is selected and visible.
    let last = settings::OPTIONS.last().unwrap().key;
    assert!(sel[0].contains(last), "{} in {:?}", last, sel[0]);
    for _ in 0..40 {
        b.on_key(key(Key::Up));
    }
    assert!(text(&b).iter().any(|l| l.contains("│> general.homepage")));
}

#[test]
fn clicking_a_setting_changes_it() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ctrl('p'));
    type_text(&mut b, "tabs.show");
    let y = row_with(&b, "tabs.show").unwrap();
    // Wait: the row with the search text also holds the query; use the list row.
    let list_y = text(&b)
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("tabs.show"))
        .map(|(i, _)| i)
        .max()
        .unwrap();
    assert!(list_y >= y);
    b.on_mouse(left(10, list_y));
    assert_eq!(b.settings().str("tabs.show"), "multiple");
    // click outside closes
    b.on_mouse(left(0, 0));
    assert!(!b.settings_open());
}

#[test]
fn about_settings_link_opens_the_box() {
    let mut b = mk(&[]);
    // the newtab page has a link to about:settings
    let y = row_with(&b, "Open settings").unwrap();
    b.on_mouse(left(1, y));
    assert!(b.settings_open());
    assert_eq!(
        b.tab_url(0).as_deref(),
        Some("about:newtab"),
        "the tab did not navigate"
    );
}

// ── profile ─────────────────────────────────────────────────────────

fn profile_at(dir: &PathBuf, name: &str) -> Profile {
    let (p, _) = Profile::load_or_new(&dir.join(name), "test").unwrap();
    p
}

#[test]
fn ctrl_s_in_the_box_saves_settings_tabs_and_applications() {
    let dir = tmp("save");
    let path = dir.join("p.toml");
    let mut b = browser_with(profile_at(&dir, "p.toml"), &["/a.cre", "/b.cre"]);
    b.on_key(ctrl('p'));
    type_text(&mut b, "tabs.position");
    b.on_key(key(Key::Enter));
    b.on_key(ctrl('s'));
    assert!(contains(&b, "saved to"), "{:?}", text(&b));
    assert!(!b.settings_unsaved());
    let (p, w) = Profile::load(&path).unwrap();
    assert!(w.is_empty());
    assert_eq!(p.settings.str("tabs.position"), "bottom");
    assert_eq!(
        p.tabs.iter().map(|t| t.url.as_str()).collect::<Vec<_>>(),
        vec!["file:///a.cre", "file:///b.cre"]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn session_only_settings_are_not_written_at_quit_but_tabs_are() {
    let dir = tmp("autosave");
    let path = dir.join("p.json");
    let mut b = browser_with(profile_at(&dir, "p.json"), &["/a.cre"]);
    b.on_key(ctrl('p'));
    type_text(&mut b, "tabs.position");
    b.on_key(key(Key::Enter));
    b.on_key(key(Key::Esc));
    b.on_key(ctrl('t'));
    b.on_key(ch('q'));
    assert!(b.should_quit());
    let (p, _) = Profile::load(&path).unwrap();
    assert_eq!(
        p.settings.str("tabs.position"),
        "top",
        "not saved on purpose"
    );
    assert_eq!(p.tabs.len(), 2);
    assert_eq!(p.active_tab, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn saved_tabs_come_back_unless_addresses_are_given() {
    let dir = tmp("restore");
    {
        let mut b = browser_with(profile_at(&dir, "p.yaml"), &["/a.cre", "/b.cre"]);
        b.on_key(with(Key::Right, false, true, false));
        b.on_key(ch('q'));
    }
    let mut b = Browser::new(
        profile_at(&dir, "p.yaml"),
        Registry::in_memory(),
        Rc::new(files()),
        60,
        20,
    );
    b.start(&[]);
    assert_eq!(b.tab_count(), 2);
    assert_eq!(b.active_tab(), 1);
    assert_eq!(b.tab_title(0), Some("Page A"));
    let mut b = Browser::new(
        profile_at(&dir, "p.yaml"),
        Registry::in_memory(),
        Rc::new(files()),
        60,
        20,
    );
    b.start(&["/c.cre".to_string()]);
    assert_eq!(b.tab_count(), 1);
    assert_eq!(b.tab_title(0), Some("Page C"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_profile_without_a_file_cannot_save() {
    let mut b = mk(&["/a.cre"]);
    b.on_key(ctrl('s'));
    assert!(b.status().contains("no file"), "{}", b.status());
    b.on_key(ch('q'));
    assert!(b.should_quit());
}

// ── plugins and pages ───────────────────────────────────────────────

#[test]
fn installed_plugins_run_on_matching_pages() {
    let reg = Registry::in_memory();
    let mut b = Browser::new(Profile::new("t").unwrap(), reg, Rc::new(files()), 60, 20);
    // A page installs a plugin for its own application; it runs on the next load.
    let mut m = files();
    m.0.insert(
        "https://x.y/app/setup".to_string(),
        "root.plugin.install(\"app\", Plugin::source(\"p \\\"from plugin\\\"\"))\np \"setup\""
            .to_string(),
    );
    m.0.insert("https://x.y/app/page".to_string(), "p \"page\"".to_string());
    m.0.insert("https://x.y/other".to_string(), "p \"other\"".to_string());
    b.loader = Rc::new(m);
    b.start(&["https://x.y/app/setup".to_string()]);
    assert!(contains(&b, "setup") && !contains(&b, "from plugin"));
    b.on_key(ctrl('l'));
    b.on_key(ctrl('u'));
    type_text(&mut b, "https://x.y/app/page");
    b.on_key(key(Key::Enter));
    assert!(
        contains(&b, "page") && contains(&b, "from plugin"),
        "{:?}",
        text(&b)
    );
    b.on_key(ctrl('l'));
    b.on_key(ctrl('u'));
    type_text(&mut b, "https://x.y/other");
    b.on_key(key(Key::Enter));
    assert!(contains(&b, "other") && !contains(&b, "from plugin"));
    // The new tab page lists the application.
    b.on_key(ctrl('t'));
    assert!(contains(&b, "x.y/app"), "{:?}", text(&b));
}

#[test]
fn a_broken_page_shows_an_error_page_with_the_message() {
    let mut m = files();
    m.0.insert("file:///bad.cre".to_string(), "blink \"x\"".to_string());
    let mut b = Browser::new(
        Profile::new("t").unwrap(),
        Registry::in_memory(),
        Rc::new(m),
        60,
        20,
    );
    b.start(&["/bad.cre".to_string()]);
    assert!(contains(&b, "CANNOT OPEN THIS PAGE"));
    assert!(contains(&b, "unknown variable `blink`"));
}

#[test]
fn error_messages_with_special_characters_are_shown_literally() {
    let mut m = files();
    m.0.insert("file:///bad.cre".to_string(), "p \"{oops\"".to_string());
    let mut b = Browser::new(
        Profile::new("t").unwrap(),
        Registry::in_memory(),
        Rc::new(m),
        60,
        20,
    );
    b.start(&["/bad.cre".to_string()]);
    assert!(contains(&b, "CANNOT OPEN THIS PAGE"));
    assert!(contains(&b, "missing `}`"), "{:?}", text(&b));
}

#[test]
fn pages_helpers_quote_anything() {
    for s in [
        "plain",
        "with \"quotes\"",
        "a \"# b",
        "braces {x} }",
        "multi\nline",
        "",
    ] {
        let src = format!("p {}", pages::raw(s));
        let rows = engine::render_source(&src, 40).unwrap();
        let shown: String = rows.iter().map(engine::row_text).collect::<String>();
        assert_eq!(shown.trim_end(), s.replace('\n', " ").trim_end(), "{:?}", s);
    }
}

// ── screen ──────────────────────────────────────────────────────────

#[test]
fn resizing_relayouts_every_tab() {
    let mut b = mk(&["/a.cre", "/b.cre"]);
    b.on_resize(30, 10);
    let s = b.screen();
    assert_eq!((s.cols, s.rows), (30, 10));
    assert_eq!(b.viewer().width, 30);
    assert_eq!(b.viewer().view_h(), 8);
    b.on_key(with(Key::Right, false, true, false));
    assert_eq!(b.viewer().width, 30);
}

#[test]
fn size_max_fills_the_window() {
    let mut m = files();
    m.0.insert(
        "file:///fill.cre".to_string(),
        "box.size:max.bcolor:blue { p \"x\" }".to_string(),
    );
    let mut b = Browser::new(
        Profile::new("t").unwrap(),
        Registry::in_memory(),
        Rc::new(m),
        20,
        10,
    );
    b.start(&["/fill.cre".to_string()]);
    let s = b.screen();
    // tab bar (1) + page (8) + status (1): the box covers all page rows and the full width
    for y in 1..9 {
        for x in 0..20 {
            assert!(s.get(x, y).unwrap().bg.is_some(), "({}, {})", x, y);
        }
    }
    assert!(s.get(0, 9).unwrap().bg.is_none());
}

#[test]
fn headers_are_scaled_only_when_the_frontend_can_draw_them() {
    let mut m = files();
    m.0.insert(
        "file:///h.cre".to_string(),
        "h1 \"Hi\"\np \"x\"".to_string(),
    );
    let mut b = Browser::new(
        Profile::new("t").unwrap(),
        Registry::in_memory(),
        Rc::new(m),
        20,
        12,
    );
    b.start(&["/h.cre".to_string()]);
    let s = b.screen();
    assert!(
        (0..20).all(|x| s.get(x, 1).unwrap().scale == 1),
        "terminal: no scaling"
    );
    b.set_capabilities(Capabilities {
        windows: false,
        scaling: true,
    });
    let s = b.screen();
    let big = s.get(0, 1).unwrap();
    assert_eq!((big.ch, big.scale, big.bold), ('H', 3, true));
    assert!(s.get(1, 1).unwrap().cover && s.get(2, 1).unwrap().cover);
    assert_eq!(s.get(3, 1).unwrap().ch, 'I');
    // the glyph spans three rows: the page text starts after them
    assert!(s.row_text(4).starts_with('x'), "{:?}", text(&b));
}

#[test]
fn the_cursor_is_painted_reversed_on_the_screen() {
    let b = mk(&["/a.cre"]);
    let y = row_with(&b, "to b").unwrap();
    let s = b.screen();
    assert!(s.get(0, y).unwrap().reverse);
    assert!(!s.get(0, y + 1).unwrap().reverse);
}

// ── the example pages ───────────────────────────────────────────────

#[test]
fn the_plugins_example_installs_lists_runs_and_clears() {
    let mut m = files();
    m.0.insert(
        "http://example.com/demo/plugins.cre".to_string(),
        include_str!("../../../examples/plugins.cre").to_string(),
    );
    let mut b = Browser::new(
        Profile::new("t").unwrap(),
        Registry::in_memory(),
        Rc::new(m),
        70,
        24,
    );
    b.start(&["http://example.com/demo/plugins.cre".to_string()]);
    b.take_effects();
    assert!(contains(&b, "Nothing installed yet."), "{:?}", text(&b));
    b.on_key(key(Key::Enter)); // install
    assert!(contains(&b, "Installed "), "{:?}", text(&b));
    b.on_key(ch('j'));
    b.on_key(key(Key::Enter)); // list
    assert!(contains(&b, "1 plugin(s) installed"), "{:?}", text(&b));
    b.on_key(ch('j'));
    b.on_key(key(Key::Enter)); // one-shot
    assert!(contains(&b, "one-shot plugin ran"), "{:?}", text(&b));
    // The installed plugin runs on the next load of a page under the application.
    b.on_key(ch('r'));
    assert!(contains(&b, "Added by a plugin"), "{:?}", text(&b));
    // Reloading puts the cursor back on the first button: the fourth is "Remove them again".
    for _ in 0..3 {
        b.on_key(ch('j'));
    }
    b.on_key(key(Key::Enter)); // clear
    b.on_key(ch('r'));
    assert!(!contains(&b, "Added by a plugin"), "{:?}", text(&b));
}

#[test]
fn the_demo_example_button_runs_its_function() {
    let mut m = files();
    m.0.insert(
        "file:///demo.cre".to_string(),
        include_str!("../../../examples/demo.cre").to_string(),
    );
    let mut b = Browser::new(
        Profile::new("t").unwrap(),
        Registry::in_memory(),
        Rc::new(m),
        70,
        40,
    );
    b.start(&["/demo.cre".to_string()]);
    assert!(contains(&b, "Press the button"));
    // Focus order: two links, the inputs, then the button.
    for _ in 0..4 {
        b.on_key(ch('j'));
    }
    b.on_key(key(Key::Enter));
    assert!(contains(&b, "Hello, friend!"), "{:?}", text(&b));
}
