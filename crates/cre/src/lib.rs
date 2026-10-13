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
mod element;
mod macros;
mod page;
mod root;

pub use attr::{Attr, Border, Color, Handler, InputType, Pad};
pub use element::{Element, Tag, take_problems};
pub use page::LivePage;
pub use root::{Root, Tree};

#[cfg(test)]
mod tests;
