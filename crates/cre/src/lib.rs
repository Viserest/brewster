//! The page SDK. A page is a Rust file that builds an [`Element`] with the tag macros and
//! exports it with [`page!`]. The browser compiles it to wasm, runs it in a sandbox and
//! reads the finished tree back (see [`abi`]).
//!
//! ```ignore
//! use cre::*;
//! page!(page);
//! fn page() -> Element {
//!     bx!({ bcolor: "#181818", pad: "1,2" }, [
//!         h1!("Hello"),
//!         button!({ onclick: |this: &Element, _root: &Root| this.set_text("Clicked") }, "Press"),
//!     ])
//! }
//! ```
//!
//! Only plain `macro_rules!` and a prebuilt rlib are involved, so a page compiles with
//! `rustc` alone: no cargo, no proc macros, no other crates.

pub mod abi;
mod attr;
mod codec;
mod element;
pub mod form;
mod macros;
pub mod net;
mod page;
pub mod route;
mod root;
mod task;
mod url;

pub use attr::{Attr, Border, Color, Fade, Handler, InputType, Pad, Radius};
pub use element::{Element, Tag, take_problems};
pub use form::FormValue;
pub use net::{
    Net, NetRequest, NetResult, NetworkError, Response, ResponseFuture, decode_requests,
    decode_result, encode_requests, encode_result,
};
pub use page::{LivePage, NoRoot, PageFn, WithRoot};
pub use root::{Root, Tree, root, spawn};
pub use route::Request;
pub use url::{Method, Url, decode_component, encode_component};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_runtime;
