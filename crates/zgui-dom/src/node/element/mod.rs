//! The four things only an element has: a name, an identifier, classes and interaction state.

pub mod classes;
pub mod ident;
pub mod name;
pub mod state;

pub use crate::node::element::classes::ClassSpan;
pub use crate::node::element::ident::IdentTable;
pub use crate::node::element::name::ElementName;
pub use crate::node::element::state::{from_engine, to_engine};
