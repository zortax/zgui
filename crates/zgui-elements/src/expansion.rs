//! The crates a `view!` expansion names.
//!
//! Nothing here is written by hand. A view written as nested tags becomes ordinary calls on the
//! view layer and on this vocabulary, and those calls have to name the crates they come from —
//! which they do through one root, so that a crate hosting a view depends on the one crate that
//! offers this module rather than on each crate the expansion happens to touch.
//!
//! A crate that writes views over this vocabulary with no umbrella over it supplies the root
//! itself:
//!
//! ```
//! extern crate zgui_elements as zgui;
//! # fn main() {}
//! ```

/// The element vocabulary.
pub use crate as elements;
/// The view layer.
pub use zgui_view::expansion::view;
