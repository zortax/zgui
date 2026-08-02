//! Writing values onto elements, and keeping them written.
//!
//! Whether an attribute is static or dynamic is decided by its type. A literal is written once,
//! at build time, with no reactive node behind it; a signal or a closure gets exactly one
//! [`Binding`], which compares against what it last wrote and touches the backend only when the
//! value actually changed.

mod a11y;
mod attrs;
mod classes;
mod effect;
mod write;

pub use crate::binding::a11y::A11yBinding;
pub use crate::binding::attrs::{AttrEntry, Attrs};
pub use crate::binding::classes::Classes;
pub use crate::binding::effect::Binding;
pub use crate::binding::write::{
    bind_attribute, bind_class, bind_custom_property, bind_custom_state, bind_property,
    bind_semantics, bind_style_property, bind_ui_state,
};
