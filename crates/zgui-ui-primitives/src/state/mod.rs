//! State a component may own, or the caller may.
//!
//! | Module | Contents |
//! |---|---|
//! | [`binding`] | [`Binding`]: what a caller tied a value to, and what a click therefore does |
//! | [`controllable`] | [`Controllable`]: the value, read and written the same way whoever owns it |

pub mod binding;
pub mod controllable;

pub use crate::state::binding::Binding;
pub use crate::state::controllable::Controllable;
