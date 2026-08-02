//! The crates a `view!` expansion names.
//!
//! Nothing here is written by hand. A view written as nested tags becomes ordinary calls on this
//! crate and on the element vocabulary, and those calls have to name the crates they come from —
//! which they do through one root, so that a crate hosting a view depends on the one crate that
//! offers this module rather than on each crate the expansion happens to touch.
//!
//! A crate that writes views without an umbrella over it supplies the root itself:
//!
//! ```
//! extern crate zgui_view as zgui;
//! # fn main() {}
//! ```

/// The view layer.
pub use crate as view;
