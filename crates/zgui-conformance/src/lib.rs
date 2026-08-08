//! CSS parity as a measurement: what this framework supports, what the evidence for each answer
//! is, and a suite whose pass rate can only go up.
//!
//! "Full CSS support" is not a claim that can be true or false — it is a fraction, and a fraction
//! needs a denominator, a classification of every entry under it, and something that fails when the
//! fraction goes down. This crate is all three, and it is never shipped: it measures the framework
//! and is not part of it.
//!
//! | Module | What it answers |
//! |---|---|
//! | [`census`] | which longhands the engine generated, and which of them nobody has classified |
//! | [`stanza`] | what the engine's own property definitions say about a property |
//! | [`crosscheck`] | whether a register row's stated reason and the definition agree |
//! | [`evidence`] | whether a property claimed *implemented* actually changes anything |
//! | [`lint`] | whether an author wrote a property that nothing acted on |
//! | [`zdoc`] | a converted conformance test: a viewport, a style sheet and a tree |
//! | [`fragment`] | the fragment tree as stable text |
//! | [`wpt::suite`] | running one converted test against its reference |
//! | [`wpt`] | converting a reference suite into [`zdoc`]s, and refusing what it cannot convert |
//! | [`ratchet`] | the per-suite pass rate, and the rule that it may never fall |
//! | [`report`] | the whole measurement, as the document CI publishes |
//!
//! # The failure this crate is arranged against
//!
//! A register of properties is a set of *claims*, and a claim costs nothing to make. The way a
//! parity number goes wrong is not that someone writes a false row on purpose — it is that a row
//! written while a consumer was planned stays behind when the consumer does not land, and nothing
//! ever asks it again. Three separate instruments here exist only to ask:
//!
//! * [`census`] asks the engine what exists, so a property nobody classified is a failure rather
//!   than a silence;
//! * [`crosscheck`] asks the engine's own property definitions why a property is unreachable, so a
//!   row that guessed the reason is a failure rather than prose;
//! * [`evidence`] runs the framework twice with the property set differently and requires the
//!   result to differ, so a row that says *implemented* while nothing consumes the value is a
//!   failure rather than a number.
//!
//! ```
//! use zgui_conformance::census::Census;
//!
//! let census = Census::take();
//! assert!(census.unclassified().is_empty(), "{:?}", census.unclassified());
//! assert!(census.implemented() > 0);
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod census;
pub mod crosscheck;
pub mod evidence;
pub mod fragment;
pub mod lint;
pub mod ratchet;
pub mod report;
pub mod stanza;
pub mod wpt;
pub mod zdoc;

pub use crate::census::Census;
pub use crate::lint::IgnoredLint;
pub use crate::zdoc::Zdoc;

/// Every declaration in the workspace, from the crates that consume properties and from the
/// backlog of those nobody has claimed.
///
/// The set is defined by the crates that declare, not by this function: a consuming crate's
/// `parity::REGISTERED` and [`zgui_css::parity::backlog`] between them are the whole of it. What a
/// caller assembling its own has to get right is therefore only that it named every source, and
/// [`zgui_css::parity::Registry::unclassified`] is what fails when it did not — a forgotten source
/// leaves longhands with no declaration at all.
pub fn registrations() -> Vec<zgui_css::parity::Registration> {
    [
        zgui_style::parity::REGISTERED.to_vec(),
        zgui_text_style::parity::REGISTERED.to_vec(),
        zgui_layout::parity::registered(),
        zgui_paint::parity::registered(),
        zgui_runtime::parity::REGISTERED.to_vec(),
        zgui_css::parity::gap::inherited_svg::REGISTERED.to_vec(),
        zgui_css::parity::backlog::registered(),
    ]
    .concat()
}
