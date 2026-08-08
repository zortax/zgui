//! CSS parity, as a number rather than a claim.
//!
//! "Full CSS support" is not a thing that can be true or false; it is a count, and a count needs a
//! denominator and a way of classifying every entry under it. This module is that instrument, in
//! three parts.
//!
//! **The declarations.** [`register_property!`](crate::register_property) puts one longhand's
//! treatment — [`Support::Implemented`], [`Support::Ignored`] or [`Support::Absent`] — in the
//! module that reads the property. Nothing central is maintained, because a central table survives
//! exactly until the first refactor.
//!
//! **The check.** [`Registration::check`] asks the engine, as built and configured right now, what
//! it says about the property, and fails when the answer contradicts the declaration. That is what
//! makes a declaration a claim instead of a comment: a preference flip or a patched engine turns a
//! row that used to be right into a test failure.
//!
//! **The register.** [`GAPS`] is the known-unreachable set — what this build cannot do, why, what a
//! fix would cost and who would carry it. Every row carries a [`GapProbe`], so a row that has
//! silently become untrue fails too.
//!
//! ```
//! use zgui_css::parity::{GAPS, Registry};
//! use zgui_css::parity::gap::inherited_svg;
//!
//! // Every row still describes this build.
//! assert!(GAPS.iter().all(|gap| gap.holds()));
//!
//! // And every declaration still matches what the engine says.
//! let mut registry = Registry::new();
//! registry.extend(inherited_svg::REGISTERED).expect("no row declared twice");
//! assert!(registry.check().is_empty());
//! ```
//!
//! # Why a longhand nobody has declared is the interesting case
//!
//! The three treatments are all *answers*. A longhand with no declaration at all is the absence of
//! one, and from the outside it is indistinguishable from a property that works — right up to the
//! moment an author writes it. [`Registry::unclassified`] is the question a parity gate asks, and
//! the denominator it is asked against is every longhand the engine generates — which is
//! [`catalog::longhands`], read out of the engine's own build rather than written down.
//!
//! [`observe`] is the same idea seen at run time: it reads any property off any computed style by
//! name, which is what lets a pass over a frame's styles notice that an author wrote something
//! nothing acted on.

pub mod backlog;
pub mod catalog;
pub mod declare;
pub mod engine;
pub mod gap;
pub mod observe;
pub mod probe;
pub mod record;
pub mod registry;
pub mod support;

pub use crate::parity::catalog::Longhand;
pub use crate::parity::engine::{EngineStatus, status_of};
pub use crate::parity::gap::{GAPS, Gap, GapProbe, GapStatus};
pub use crate::parity::probe::{complaints, media_feature_is_accepted, selector_is_accepted};
pub use crate::parity::record::{ParityError, Registration};
pub use crate::parity::registry::{Conflict, Counts, Disagreement, Registry};
pub use crate::parity::support::{AbsentReason, Expectation, Support};
