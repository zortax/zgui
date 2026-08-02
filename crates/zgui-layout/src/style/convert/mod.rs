//! Computed values, restated as the layout algorithms want them.
//!
//! Every conversion here is a pure function of one computed value and the pass's scale factor, so
//! each is testable on its own and none needs a tree, a document or a cascade behind it.

pub mod align;
pub mod aspect;
pub mod display;
pub mod length;
pub mod overflow;
