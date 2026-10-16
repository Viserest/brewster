//! The address, `root()`, the network, routes and forms.

use super::*;

fn click_button(page: &LivePage) {
    let id = page.root().tree.query("button").unwrap().id();
    assert!(page.click(id));
}

fn text_of(page: &LivePage, selector: &str) -> String {
    page.root().tree.query(selector).unwrap().text()
}

fn ok(status: u16, body: &str) -> NetResult {
    Ok(Response {
        status,
        headers: vec![("Content-Type".into(), "text/plain".into())],
        body: body.as_bytes().to_vec(),
    })
}

// ── addresses ───────────────────────────────────────────────────────

#[test]
fn urls_are_taken_apart() {
    let u = Url::parse("http://host.org:8080/a/b?x=1&y=two%20words&z#top");
    assert_eq!(
        (u.scheme.as_str(), u.authority.as_str(), u.path.as_str()),
        ("http", "host.org:8080", "/a/b")
    );
    assert_eq!(u.query.as_deref(), Some("x=1&y=two%20words&z"));
    assert_eq!(u.fragment.as_deref(), Some("top"));
    assert_eq!(u.method, Method::Get);
    assert_eq!(
        u.query_pairs(),
        [("x".into(), "1".into()), ("y".into(), "two words".into()), ("z".into(), "".into())]
    );
    assert_eq!(u.query_param("y").as_deref(), Some("two words"));
    assert_eq!(u.query_param("nope"), None);
    assert_eq!(u.to_string(), "http://host.org:8080/a/b?x=1&y=two%20words&z#top");

    assert_eq!(Url::parse("http://h.org").path, "/");
    let f = Url::parse("file:///home/me/page.rs");
    assert_eq!((f.scheme.as_str(), f.authority.as_str(), f.path.as_str()), ("file", "", "/home/me/page.rs"));
    let a = Url::parse("about:newtab");
    assert_eq!((a.scheme.as_str(), a.path.as_str()), ("about", "newtab"));
    let bare = Url::parse("/just/a/path?q=1");
    assert_eq!((bare.scheme.as_str(), bare.path.as_str(), bare.query.as_deref()), ("", "/just/a/path", Some("q=1")));
    // Nothing here may panic.
    for s in ["", ":", "://", "a:", "#", "?", "%", "%4", "http://", "http://?#"] {
        let _ = Url::parse(s);
    }
}

#[test]
fn components_encode_and_decode() {
    assert_eq!(encode_component("a b&c=d/é"), "a+b%26c%3Dd%2F%C3%A9");
    assert_eq!(decode_component("a+b%26c%3Dd%2F%C3%A9"), "a b&c=d/é");
    assert_eq!(decode_component("100%"), "100%");
    assert_eq!(decode_component("%zz%4"), "%zz%4");
    assert_eq!(decode_component(&encode_component("→ ünï ✓ 100% ~._-")), "→ ünï ✓ 100% ~._-");
}

#[test]
fn methods_are_read_in_any_case() {
    assert_eq!(Method::parse("get"), Some(Method::Get));
    assert_eq!(Method::parse("GET"), Some(Method::Get));
    assert_eq!(Method::parse("PoSt"), Some(Method::Post));
    for (s, m) in [("put", Method::Put), ("Patch", Method::Patch), ("DELETE", Method::Delete), ("head", Method::Head), ("options", Method::Options)] {
        assert_eq!(Method::parse(s), Some(m));
        assert_eq!(Method::parse(m.as_str()), Some(m));
    }
    assert_eq!(Method::parse("fetch"), None);
    assert_eq!(Method::parse(""), None);
    const D: Method = Method::parse_const("delete");
    assert_eq!(D, Method::Delete);
    assert_eq!(Method::Post.to_string(), "POST");
}

// ── root() ──────────────────────────────────────────────────────────

#[test]
fn root_gives_the_address_while_building_and_in_handlers() {
    let page = LivePage::at(Url::parse("http://a.b/x/y?q=1"), |root| {
        assert_eq!(root.url().path, "/x/y");
        assert_eq!(crate::root().url(), root.url());
        bx!([
            p!({ id: "where" }, root.url().href),
            button!({ onclick: |_: &Element, root: &Root| {
                root.tree.query("#where").unwrap().set_text(crate::root().url().href);
            } }, "look"),
        ])
    });
    assert_eq!(text_of(&page, "#where"), "http://a.b/x/y?q=1");
    // The tab moved: the page sees the new address, and handlers read it when they run.
    page.set_url(Url::parse("http://a.b/z"));
    click_button(&page);
    assert_eq!(text_of(&page, "#where"), "http://a.b/z");
}

#[test]
fn root_outside_a_page_is_an_error_not_a_stale_page() {
    assert!(std::panic::catch_unwind(|| root()).is_err());
    let _page = LivePage::at(Url::parse("about:blank"), |_| bx!());
    // The page is built: it is no longer the running one.
    assert!(std::panic::catch_unwind(|| root()).is_err());
}

#[test]
fn pages_that_nest_each_see_their_own_root() {
    let outer = LivePage::at(Url::parse("http://outer/"), |_| {
        let inner = LivePage::at(Url::parse("http://inner/"), |_| {
            assert_eq!(root().url().authority, "inner");
            bx!()
        });
        assert_eq!(inner.root().url().authority, "inner");
        // Back in the outer page.
        assert_eq!(root().url().authority, "outer");
        bx!()
    });
    assert_eq!(outer.root().url().authority, "outer");
}

// ── network ─────────────────────────────────────────────────────────

#[test]
fn net_messages_round_trip_and_garbage_is_an_error() {
    let reqs = vec![
        NetRequest { id: 1, method: Method::Post, url: "/login".into(), headers: vec![("A".into(), "b".into())], body: b"x=1".to_vec() },
        NetRequest { id: 2, method: Method::Get, url: "http://h/".into(), headers: vec![], body: vec![] },
    ];
    assert_eq!(decode_requests(&encode_requests(&reqs)).unwrap(), reqs);
    for r in [ok(204, ""), ok(200, "héllo"), Err(NetworkError::Blocked("x".into())), Err(NetworkError::Failed("y".into())), Err(NetworkError::Cancelled)] {
        assert_eq!(decode_result(&encode_result(&r)).unwrap(), r);
    }
    // Every cut of a valid message, and a huge claimed length, are errors.
    let good = encode_requests(&reqs);
    for n in 0..good.len() {
        assert!(decode_requests(&good[..n]).is_err(), "cut at {n}");
    }
    assert!(decode_requests(&[0xff, 0xff, 0xff, 0xff]).is_err());
    assert!(decode_result(&[9]).is_err());
    assert!(decode_result(&[]).is_err());
}

fn posting_page() -> LivePage {
    LivePage::at(Url::parse("http://a.b/"), |_| {
        bx!([
            p!({ id: "out" }, "idle"),
            button!({ onclick: |_: &Element, r: &Root| {
                r.spawn(async {
                    let res = root().net().post("/echo", b"hi".to_vec()).await;
                    let text = match res {
                        Ok(r) => format!("{} {}", r.status, r.text()),
                        Err(e) => e.to_string(),
                    };
                    crate::root().tree.query("#out").unwrap().set_text(text);
                });
            } }, "go"),
        ])
    })
}

#[test]
fn a_spawned_future_resumes_when_the_answer_arrives() {
    let page = posting_page();
    click_button(&page);
    // Asked for, not answered: the page still says idle, and one task waits.
    assert_eq!(text_of(&page, "#out"), "idle");
    assert_eq!(page.root().pending_tasks(), 1);
    let reqs = page.net_take();
    assert_eq!(reqs.len(), 1);
    assert_eq!((reqs[0].method, reqs[0].url.as_str(), reqs[0].body.as_slice()), (Method::Post, "/echo", &b"hi"[..]));
    assert!(page.net_take().is_empty(), "a request is handed over once");

    page.net_done(reqs[0].id, ok(200, "hello"));
    assert_eq!(text_of(&page, "#out"), "200 hello");
    assert_eq!(page.root().pending_tasks(), 0);
}

#[test]
fn network_errors_reach_the_page_as_values() {
    let page = posting_page();
    click_button(&page);
    let id = page.net_take()[0].id;
    page.net_done(id, Err(NetworkError::Blocked("another origin".into())));
    assert_eq!(text_of(&page, "#out"), "blocked: another origin");
    // An error status is a response, not an error.
    let page = posting_page();
    click_button(&page);
    let id = page.net_take()[0].id;
    page.net_done(id, ok(500, "boom"));
    assert_eq!(text_of(&page, "#out"), "500 boom");
}

#[test]
fn answers_for_nobody_are_ignored() {
    let page = posting_page();
    page.net_done(12345, ok(200, "x"));
    click_button(&page);
    let id = page.net_take()[0].id;
    page.net_done(id, ok(200, "once"));
    page.net_done(id, ok(200, "twice")); // the task is gone
    assert_eq!(text_of(&page, "#out"), "200 once");
}

#[test]
fn a_dropped_future_withdraws_an_unsent_request_and_cancels_a_sent_one() {
    let page = LivePage::at(Url::default(), |_| bx!());
    let net = page.root().net();
    drop(net.get("/never", vec![]));
    assert!(page.net_take().is_empty());

    let sent = net.get("/sent", vec![]);
    let id = page.net_take()[0].id;
    drop(sent);
    page.net_done(id, ok(200, "late")); // nobody is waiting: no panic
}

#[test]
fn spawned_tasks_can_spawn_and_all_of_them_run() {
    let page = LivePage::at(Url::default(), |_| bx!([p!({ id: "o" }, "")]));
    let root = page.root().clone();
    let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::<&'static str>::new()));
    let (l1, l2, r2) = (log.clone(), log.clone(), root.clone());
    root.spawn(async move {
        l1.borrow_mut().push("outer start");
        let l3 = l2.clone();
        r2.spawn(async move { l3.borrow_mut().push("inner") });
        l1.borrow_mut().push("outer end");
    });
    assert_eq!(*log.borrow(), ["outer start", "outer end", "inner"]);
    assert_eq!(root.pending_tasks(), 0);
}

#[test]
fn a_task_awaiting_two_requests_gets_both_answers() {
    let page = LivePage::at(Url::default(), |_| bx!([p!({ id: "o" }, "")]));
    let root = page.root().clone();
    root.spawn(async {
        let a = root_net_text("/a").await;
        let b = root_net_text("/b").await;
        crate::root().tree.query("#o").unwrap().set_text(format!("{a}+{b}"));
    });
    let first = page.net_take();
    assert_eq!(first[0].url, "/a");
    page.net_done(first[0].id, ok(200, "A"));
    let second = page.net_take();
    assert_eq!(second[0].url, "/b", "the second request starts after the first answer");
    page.net_done(second[0].id, ok(200, "B"));
    assert_eq!(text_of(&page, "#o"), "A+B");
}

async fn root_net_text(url: &str) -> String {
    root().net().get(url, vec![]).await.map(|r| r.text()).unwrap_or_default()
}

#[test]
fn input_values_typed_by_the_user_reach_the_page() {
    let page = LivePage::at(Url::default(), |_| bx!([input!({ id: "u" }, Text, "user", "start")]));
    let id = page.root().tree.query("#u").unwrap().id();
    assert_eq!(text_of(&page, "#u"), "start");
    assert!(page.set_input(id, "typed"));
    assert_eq!(text_of(&page, "#u"), "typed");
    // Only inputs take values.
    assert!(!page.set_input(usize::MAX, "x"));
}

// ── routes ──────────────────────────────────────────────────────────

fn m(path: &str) -> Url {
    Url::parse(&format!("http://h{path}"))
}

#[test]
fn patterns_match_paths() {
    use route::matches;
    let get = Method::Get;
    assert_eq!(matches(get, "/", &m("/")), Some(vec![]));
    assert_eq!(matches(get, "/", &m("")), Some(vec![]));
    assert_eq!(matches(get, "/health", &m("/health/")), Some(vec![]), "a trailing slash does not matter");
    assert_eq!(matches(get, "/health", &m("/healthy")), None);
    assert_eq!(matches(get, "/health", &m("/health/more")), None);
    assert_eq!(matches(get, "/hello/:name", &m("/hello/bob")), Some(vec![("name".into(), "bob".into())]));
    assert_eq!(matches(get, "/hello/:name", &m("/hello/a%20b")), Some(vec![("name".into(), "a b".into())]));
    assert_eq!(matches(get, "/hello/:name", &m("/hello")), None, "a parameter needs a segment");
    assert_eq!(matches(get, "/u/:id/p/:post", &m("/u/1/p/2?x=y")), Some(vec![("id".into(), "1".into()), ("post".into(), "2".into())]));
    assert_eq!(matches(get, "/files/*rest", &m("/files/a/b/c.txt")), Some(vec![("rest".into(), "a/b/c.txt".into())]));
    assert_eq!(matches(get, "/files/*rest", &m("/files")), Some(vec![("rest".into(), "".into())]));
    assert_eq!(matches(get, "/*rest/x", &m("/a/x")), None, "a wildcard must be last");
    // The method has to match too.
    assert_eq!(matches(Method::Post, "/", &m("/")), None);
    let mut posted = m("/");
    posted.method = Method::Post;
    assert!(matches(Method::Post, "/", &posted).is_some());
}

mod routed {
    use crate::*;

    fn index() -> Element {
        p!({ id: "r" }, "index")
    }
    fn hello(req: &Request) -> Element {
        p!({ id: "r" }, format!("hello {} {}", req.param("name").unwrap(), req.query("x").unwrap_or_default()))
    }
    fn health() -> Element {
        p!({ id: "r" }, "ok")
    }
    fn created() -> Element {
        p!({ id: "r" }, "created")
    }

    // The first and only `router!` of this test binary: it exports the page's entry points.
    router!([
        get "/": index,
        GET "/hello/:name": hello,
        Get "/health": health,
        post "/items": created,
    ]);
}

#[test]
fn router_picks_the_handler_for_the_address() {
    let at = |href: &str| {
        let page = LivePage::at(Url::parse(href), routed::__cre_router);
        page.root().tree.query("#r").map(|e| e.text()).unwrap_or_else(|| {
            // The not-found page has no #r: report its heading.
            page.root().tree.query("h2").unwrap().text()
        })
    };
    assert_eq!(at("http://h/"), "index");
    assert_eq!(at("http://h/hello/bob?x=1"), "hello bob 1");
    assert_eq!(at("http://h/health"), "ok");
    assert_eq!(at("http://h/nope"), "Not found");
    // Only a GET page is reachable by following a link: this route is for the method.
    assert_eq!(at("http://h/items"), "Not found");
    let mut posted = Url::parse("http://h/items");
    posted.method = Method::Post;
    let page = LivePage::at(posted, routed::__cre_router);
    assert_eq!(page.root().tree.query("#r").unwrap().text(), "created");
}

// ── forms ───────────────────────────────────────────────────────────

mod forms {
    use crate::*;
    form!(struct Login { username: String => Text, password: String => Password, remember: bool => Text }, post "/login");
    form!(struct Search { q: String => Text, page: u32 => Text }, GET "/search?lang=en");
}

fn form_page() -> LivePage {
    LivePage::at(Url::parse("http://a.b/"), |_| forms::Login::element())
}

fn type_into(page: &LivePage, field: &str, value: &str) {
    let id = page.root().tree.query(&format!("#form-Login-{field}")).unwrap().id();
    assert!(page.set_input(id, value));
}

#[test]
fn a_form_builds_its_inputs_button_and_status() {
    let page = form_page();
    let t = &page.root().tree;
    for (f, protected) in [("username", false), ("password", true), ("remember", false)] {
        let e = t.query(&format!("#form-Login-{f}")).unwrap();
        assert_eq!(e.tag(), "input");
        assert_eq!(e.args()[0], f, "the field name labels the input");
        assert_eq!(e.style("protected").is_some(), protected, "{f}");
    }
    assert_eq!(t.query("button").unwrap().text(), "Submit");
    assert_eq!(t.query("#form-Login-status").unwrap().text(), "");
}

#[test]
fn a_form_reads_what_was_typed_and_sends_it_encoded() {
    let page = form_page();
    type_into(&page, "username", "ann lee");
    type_into(&page, "password", "p&ss=1");
    type_into(&page, "remember", "yes");
    let login = forms::Login::read(page.root());
    assert_eq!(
        login,
        forms::Login { username: "ann lee".into(), password: "p&ss=1".into(), remember: true }
    );

    click_button(&page);
    assert_eq!(text_of(&page, "#form-Login-status"), "sending...");
    let reqs = page.net_take();
    assert_eq!(reqs.len(), 1);
    let r = &reqs[0];
    assert_eq!((r.method, r.url.as_str()), (Method::Post, "/login"));
    assert_eq!(String::from_utf8(r.body.clone()).unwrap(), "username=ann+lee&password=p%26ss%3D1&remember=true");
    assert!(r.headers.iter().any(|(k, v)| k == "Content-Type" && v == "application/x-www-form-urlencoded"));

    page.net_done(r.id, ok(200, "welcome"));
    assert_eq!(text_of(&page, "#form-Login-status"), "done (200)");
}

#[test]
fn a_form_reports_refusals_and_error_statuses() {
    let page = form_page();
    click_button(&page);
    let id = page.net_take()[0].id;
    page.net_done(id, ok(401, "no"));
    assert_eq!(text_of(&page, "#form-Login-status"), "failed: HTTP 401");

    click_button(&page);
    let id = page.net_take()[0].id;
    page.net_done(id, Err(NetworkError::Failed("connection refused".into())));
    assert_eq!(text_of(&page, "#form-Login-status"), "failed: network error: connection refused");
}

#[test]
fn call_is_a_future_for_any_code_to_await() {
    let page = LivePage::at(Url::default(), |_| bx!([p!({ id: "o" }, "")]));
    let root = page.root().clone();
    root.spawn(async {
        let r = forms::Login { username: "a".into(), password: "b".into(), remember: false }.call().await;
        crate::root().tree.query("#o").unwrap().set_text(r.unwrap().text());
    });
    let id = page.net_take()[0].id;
    page.net_done(id, ok(200, "token"));
    assert_eq!(text_of(&page, "#o"), "token");
}

#[test]
fn a_get_form_goes_in_the_query_and_numbers_are_parsed() {
    let page = LivePage::at(Url::default(), |_| {
        // `forms::Search` has no `element()` use here: build its inputs by hand to type into.
        bx!([
            input!({ id: "form-Search-q" }, Text, "q", ""),
            input!({ id: "form-Search-page" }, Text, "page", ""),
        ])
    });
    let q = page.root().tree.query("#form-Search-q").unwrap().id();
    let p = page.root().tree.query("#form-Search-page").unwrap().id();
    page.set_input(q, "rust & wasm");
    page.set_input(p, "3");
    assert_eq!(forms::Search::read(page.root()), forms::Search { q: "rust & wasm".into(), page: 3 });
    page.set_input(p, "not a number");
    assert_eq!(forms::Search::read(page.root()).page, 0, "an unparsable number is the default");

    page.set_input(p, "3");
    let root = page.root().clone();
    root.spawn(async {
        let _ = forms::Search::read(&crate::root()).call().await;
    });
    let r = &page.net_take()[0];
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.url, "/search?lang=en&q=rust+%26+wasm&page=3", "added to the query already there");
    assert!(r.body.is_empty());
}

#[test]
fn consts_describe_the_form() {
    assert_eq!(forms::Login::METHOD, Method::Post);
    assert_eq!(forms::Login::URL, "/login");
    assert_eq!(forms::Search::METHOD, Method::Get);
}

#[test]
fn form_values_convert_both_ways() {
    assert_eq!(<u32 as FormValue>::from_form(" 42 "), 42);
    assert_eq!(<i64 as FormValue>::from_form("-7"), -7);
    assert_eq!(<f64 as FormValue>::from_form("1.5"), 1.5);
    assert_eq!(<bool as FormValue>::from_form("On"), true);
    assert_eq!(<bool as FormValue>::from_form("nah"), false);
    assert_eq!(7u8.to_form(), "7");
    assert_eq!(String::from_form("x"), "x");
    assert_eq!(form::urlencode(&[("a b", "c&d".to_string())]), b"a+b=c%26d");
}
