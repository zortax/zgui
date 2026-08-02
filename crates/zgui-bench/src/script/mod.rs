//! The script every window in a comparison is driven through, event for event.
//!
//! A differential is only ever as good as the sequence it replays. Two windows that agree at every
//! step of a script that never resizes a scrolled document agree about a document neither of them
//! was ever asked the hard question about — which is how a page can lose half its content in front
//! of a reader while a comparison across four document sizes reports nothing at all.
//!
//! So the script is a *mixture*, in one sitting, and the mixture is the point. Each of the savings
//! it exists to catch is a record held across frames, and what makes one of them answer wrongly is
//! a later step asking a question in a state an earlier step put the engine in. A run of hovers on
//! its own can never catch a scroll that left a paragraph's break memo standing; a run of resizes
//! that never scrolls can never catch an offset clamped against an extent that has since moved.
//!
//! # What it is made of
//!
//! [`Step`] is the vocabulary and [`script`] is the sequence. [`run_step`] delivers one step to one
//! window, either incrementally or with every held result thrown away first, so that the same
//! sequence is what the cold window and the live window are both driven through.

mod plan;
mod run;
mod step;

use zgui::geom::{Css, CssPx, Point};

pub(crate) use self::plan::script;
pub(crate) use self::run::run_step;
#[allow(
    unused_imports,
    reason = "the sequence and the driver each name it; this is where it is spelled once"
)]
pub(crate) use self::step::Step;

/// Everything a step needs about the *window* it is driving, as opposed to the step itself.
///
/// One of these per window, and never shared: a comparison drives two windows through one script,
/// and every handle in here names something one of them owns. A script that reached for either of
/// them globally would drive one window twice and the other never.
pub(crate) struct Driven {
    /// Whether every held layout result is thrown away before each turn of the loop.
    pub(crate) cold: bool,
    /// Where this window's probe swatches are.
    pub(crate) centres: Vec<Point<CssPx, Css>>,
    /// The signal this window's own document presents its colour scheme through.
    pub(crate) scheme: crate::gallery::Scheme,
}
