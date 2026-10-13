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

/// Exports a page module's entry points. `cre::page!(page);` where
/// `fn page() -> Element`.
#[macro_export]
macro_rules! page {
    ($f:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn cre_abi() -> u32 {
            $crate::abi::VERSION
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
        pub extern "C" fn cre_problems() -> u64 {
            $crate::abi::problems()
        }
    };
}
