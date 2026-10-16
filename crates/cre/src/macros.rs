//! The tag macros. One shape for all of them:
//!
//! ```text
//! bx!( {attributes}? , [children]? )          box (`box` is a Rust keyword)
//! p!( {attributes}? , "text" )                also span!, pre!, button!, h1!..h6!
//! link!( {attributes}? , "href", "label" )
//! input!( {attributes}? , Text|Password, "name", "value" )
//! ```
//!
//! Inside `{ }` each entry is `name: value`, where `name` must be a variant of
//! [`Attr`](crate::Attr) and `value` anything that converts into its type with `Into`
//! (strings for colors, spacing and borders).

/// Shared by the macros: builds an element from a tag, attributes, text arguments and
/// children. Not for direct use.
#[doc(hidden)]
#[macro_export]
macro_rules! __element {
    ($tag:expr, {$($k:ident : $v:expr),* $(,)?}, [$($arg:expr),*], [$($c:expr),*]) => {{
        let e = $crate::Element::new($tag);
        e.set_args(::std::vec![$(::std::string::ToString::to_string(&$arg)),*]);
        $( e.set($crate::Attr::$k(::std::convert::Into::into($v))); )*
        $( e.push($c); )*
        e.finish()
    }};
}

#[macro_export]
macro_rules! bx {
    () => { $crate::__element!($crate::Tag::Box, {}, [], []) };
    ([$($c:expr),* $(,)?]) => { $crate::__element!($crate::Tag::Box, {}, [], [$($c),*]) };
    ({$($a:tt)*}) => { $crate::bx!({$($a)*}, []) };
    ({$($k:ident : $v:expr),* $(,)?}, [$($c:expr),* $(,)?]) => {
        $crate::__element!($crate::Tag::Box, {$($k: $v),*}, [], [$($c),*])
    };
}

// Text tags: `name!("text")` or `name!({attributes}, "text")`.

#[macro_export]
macro_rules! p {
    ({$($k:ident : $v:expr),* $(,)?}, $text:expr) => {
        $crate::__element!($crate::Tag::P, {$($k: $v),*}, [$text], [])
    };
    ($text:expr) => { $crate::p!({}, $text) };
}

#[macro_export]
macro_rules! span {
    ({$($k:ident : $v:expr),* $(,)?}, $text:expr) => {
        $crate::__element!($crate::Tag::Span, {$($k: $v),*}, [$text], [])
    };
    ($text:expr) => { $crate::span!({}, $text) };
}

#[macro_export]
macro_rules! pre {
    ({$($k:ident : $v:expr),* $(,)?}, $text:expr) => {
        $crate::__element!($crate::Tag::Pre, {$($k: $v),*}, [$text], [])
    };
    ($text:expr) => { $crate::pre!({}, $text) };
}

#[macro_export]
macro_rules! button {
    ({$($k:ident : $v:expr),* $(,)?}, $text:expr) => {
        $crate::__element!($crate::Tag::Button, {$($k: $v),*}, [$text], [])
    };
    ($text:expr) => { $crate::button!({}, $text) };
}

#[macro_export]
macro_rules! h1 {
    ({$($k:ident : $v:expr),* $(,)?}, $text:expr) => {
        $crate::__element!($crate::Tag::H(1), {$($k: $v),*}, [$text], [])
    };
    ($text:expr) => { $crate::h1!({}, $text) };
}

#[macro_export]
macro_rules! h2 {
    ({$($k:ident : $v:expr),* $(,)?}, $text:expr) => {
        $crate::__element!($crate::Tag::H(2), {$($k: $v),*}, [$text], [])
    };
    ($text:expr) => { $crate::h2!({}, $text) };
}

#[macro_export]
macro_rules! h3 {
    ({$($k:ident : $v:expr),* $(,)?}, $text:expr) => {
        $crate::__element!($crate::Tag::H(3), {$($k: $v),*}, [$text], [])
    };
    ($text:expr) => { $crate::h3!({}, $text) };
}

#[macro_export]
macro_rules! h4 {
    ({$($k:ident : $v:expr),* $(,)?}, $text:expr) => {
        $crate::__element!($crate::Tag::H(4), {$($k: $v),*}, [$text], [])
    };
    ($text:expr) => { $crate::h4!({}, $text) };
}

#[macro_export]
macro_rules! h5 {
    ({$($k:ident : $v:expr),* $(,)?}, $text:expr) => {
        $crate::__element!($crate::Tag::H(5), {$($k: $v),*}, [$text], [])
    };
    ($text:expr) => { $crate::h5!({}, $text) };
}

#[macro_export]
macro_rules! h6 {
    ({$($k:ident : $v:expr),* $(,)?}, $text:expr) => {
        $crate::__element!($crate::Tag::H(6), {$($k: $v),*}, [$text], [])
    };
    ($text:expr) => { $crate::h6!({}, $text) };
}

#[macro_export]
macro_rules! link {
    ({$($k:ident : $v:expr),* $(,)?}, $href:expr, $label:expr) => {
        $crate::__element!($crate::Tag::Link, {$($k: $v),*}, [$href, $label], [])
    };
    ($href:expr, $label:expr) => { $crate::link!({}, $href, $label) };
}

#[macro_export]
macro_rules! input {
    ($ty:ident, $name:expr, $value:expr) => { $crate::input!({}, $ty, $name, $value) };
    ({$($k:ident : $v:expr),* $(,)?}, $ty:ident, $name:expr, $value:expr) => {{
        let e = $crate::__element!($crate::Tag::Input, {$($k: $v),*}, [$name, $value], []);
        e.set_input_type($crate::InputType::$ty);
        e
    }};
}

/// Exports a page module's entry points. `cre::page!(page);` where `page` is
/// `fn() -> Element` or `fn(&Root) -> Element` (the root gives the page its address).
#[macro_export]
macro_rules! page {
    ($f:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn cre_abi() -> u32 {
            $crate::abi::VERSION
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn cre_alloc(len: u32) -> u32 {
            $crate::abi::alloc(len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn cre_set_url(len: u32) {
            $crate::abi::set_url(len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn cre_page() -> u64 {
            $crate::abi::page($f)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn cre_event(id: u32) -> u32 {
            $crate::abi::event(id)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn cre_input(id: u32, len: u32) {
            $crate::abi::set_input(id, len)
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn cre_problems() -> u64 {
            $crate::abi::problems()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn cre_net_take() -> u64 {
            $crate::abi::net_take()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn cre_net_done(id: u32, len: u32) {
            $crate::abi::net_done(id, len)
        }
    };
}

/// Routes by the page's address (`root().url()`) and exports the result as the page:
///
/// ```ignore
/// cre::router!([
///     get "/": index,
///     get "/hello/:name": hello,   // fn hello(req: &Request) -> Element
///     get "/health": health,       // fn health() -> Element
/// ]);
/// ```
///
/// The method may be written in any case (`get`, `GET`); an unknown one does not compile.
/// A pattern's `:name` takes one path segment and a last `*name` takes the rest; a handler
/// is `fn() -> Element` or `fn(&Request) -> Element`. The first route that matches wins; no
/// match shows a "Not found" page.
#[macro_export]
macro_rules! router {
    ([ $( $method:ident $path:literal : $handler:path ),* $(,)? ]) => {
        #[doc(hidden)]
        pub fn __cre_router(root: &$crate::Root) -> $crate::Element {
            let url = root.url();
            $(
                {
                    // A compile error for a method that does not exist.
                    const METHOD: $crate::Method = $crate::Method::parse_const(stringify!($method));
                    if let Some(params) = $crate::route::matches(METHOD, $path, &url) {
                        let req = $crate::Request { url: url.clone(), params };
                        return $crate::route::call($handler, &req);
                    }
                }
            )*
            $crate::route::not_found(&url)
        }
        $crate::page!(__cre_router);
    };
}

/// Declares a form: a struct of fields, how each is entered, and where it goes.
///
/// ```ignore
/// cre::form!(struct Login { username: String => Text, password: String => Password }, post "/login");
/// ```
///
/// makes `struct Login { pub username: String, pub password: String }` and
///
/// * `Login::element() -> Element`: a box with an input per field, a submit button and a
///   status line;
/// * `Login::read(&Root) -> Login`: the values currently in those inputs;
/// * `login.call() -> impl Future<Output = Result<Response, NetworkError>>`: sends the
///   fields to the URL with the method (form-encoded: in the body, or in the query for
///   `get`), through `root().net()`;
/// * `Login::submit(&Root)`: reads the form and sends it in the background, writing the
///   outcome to the status line (what the submit button does).
///
/// A field's type is anything that is a [`FormValue`](crate::FormValue): text, numbers,
/// `bool`. Its input type is `Text` or `Password`.
#[macro_export]
macro_rules! form {
    (struct $name:ident { $($field:ident : $ty:ty => $input:ident),* $(,)? }, $method:ident $url:literal) => {
        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct $name {
            $(pub $field: $ty),*
        }

        // Checked even when nothing uses `METHOD`: an unknown method does not compile.
        const _: $crate::Method = $crate::Method::parse_const(stringify!($method));

        #[allow(dead_code)]
        impl $name {
            pub const METHOD: $crate::Method = $crate::Method::parse_const(stringify!($method));
            pub const URL: &'static str = $url;

            /// The form: an input per field, a submit button and a status line.
            pub fn element() -> $crate::Element {
                let e = $crate::bx!([
                    $(
                        $crate::input!(
                            { id: concat!("form-", stringify!($name), "-", stringify!($field)) },
                            $input,
                            stringify!($field),
                            ""
                        )
                    ),*
                ]);
                e.push($crate::button!(
                    { onclick: |_this: &$crate::Element, root: &$crate::Root| { $name::submit(root) } },
                    "Submit"
                ));
                e.push($crate::p!(
                    { id: concat!("form-", stringify!($name), "-status") },
                    ""
                ));
                e
            }

            /// The values currently in the form's inputs.
            pub fn read(root: &$crate::Root) -> Self {
                Self {
                    $(
                        $field: <$ty as $crate::FormValue>::from_form(
                            &root
                                .tree
                                .query(concat!("#form-", stringify!($name), "-", stringify!($field)))
                                .map(|e| e.text())
                                .unwrap_or_default(),
                        )
                    ),*
                }
            }

            /// Sends the fields. Starts the request now; await the result.
            pub fn call(&self) -> $crate::ResponseFuture {
                $crate::form::send(
                    Self::METHOD,
                    Self::URL,
                    &[$((stringify!($field), $crate::FormValue::to_form(&self.$field))),*],
                )
            }

            /// Reads the form and sends it in the background; the outcome goes to the
            /// status line.
            pub fn submit(root: &$crate::Root) {
                let form = Self::read(root);
                let status = root.tree.query(concat!("#form-", stringify!($name), "-status"));
                if let Some(s) = &status {
                    s.set_text("sending...");
                }
                let pending = form.call();
                root.spawn(async move {
                    let result = pending.await;
                    if let Some(s) = status {
                        s.set_text($crate::form::describe(&result));
                    }
                });
            }
        }
    };
}
