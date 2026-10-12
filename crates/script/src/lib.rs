//! Scripting: the interpreter behind `.cre` files, `root.tree`, `root.plugin`, buttons'
//! `onclick` and the page lifecycle ([`Page`]).

mod interp;
mod methods;
mod page;
mod value;

pub use interp::{Interpreter, STEP_BUDGET, Services};
pub use page::Page;
pub use value::{Scope, Value, values_equal};

#[cfg(test)]
mod tests;
