//! The icons themselves, grouped by what they are for.
//!
//! Every icon is a `const` [`IconData`](crate::IconData) drawn in a 24-unit square, filled with
//! the non-zero rule. A program links exactly the ones it names.
//!
//! | Module | Contents |
//! |---|---|
//! | [`arrow`] | the four arrows |
//! | [`chevron`] | the four chevrons |
//! | [`mark`] | check, minus, plus, cross, disc, dot |
//! | [`status`] | the circled and triangular status marks |
//! | [`ui`] | search, spinner, ellipsis |

pub mod arrow;
pub mod chevron;
pub mod mark;
pub mod status;
pub mod ui;
