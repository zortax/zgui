//! Whether a property claimed *implemented* actually changes anything.
//!
//! # The defect this exists to catch
//!
//! A register row saying a property is consumed is a claim, and the way such a claim goes wrong is
//! not dishonesty: a row written while a consumer was being planned outlives the plan, the consumer
//! lands somewhere else or not at all, and nothing ever asks the row again. Review has caught
//! exactly that — rows recorded as implemented with nothing reading the value — and a register
//! whose whole purpose is to prevent silent parity claims cannot be the thing making them.
//!
//! So a row here is not believed. Every property claimed implemented is written into a fixture,
//! the fixture is laid out twice — once without the declaration, once with it — and the two
//! fragment trees are compared. A property whose value nothing acts on produces two identical
//! trees, and that is a failure.
//!
//! # The three answers, and why the middle one matters
//!
//! * [`Verdict::Changed`] — the declaration moved something a stage after layout reads. Proven.
//! * [`Verdict::Unchanged`] — the cascade took the declaration and nothing downstream did anything
//!   with it. This is the over-claim.
//! * [`Verdict::Inert`] — the declaration did not even reach a computed style, so the probe is
//!   broken and proves nothing either way. Reported separately and never counted as evidence,
//!   because a probe that silently stopped working would turn every row it covers green.
//!
//! ```
//! use zgui_conformance::evidence::{Probe, Verdict};
//!
//! // Making the text bigger moves every line it is on.
//! assert_eq!(Probe::new("font_size", "font-size: 40px").run(), Verdict::Changed);
//!
//! // Naming a family the deterministic shaper does not distinguish moves nothing, even though the
//! // cascade took the declaration.
//! assert_eq!(Probe::new("font_kerning", "font-kerning: none").run(), Verdict::Unchanged);
//! ```

pub mod fixture;
pub mod probe;
pub mod probes;
pub mod survey;
pub mod unproven;

pub use crate::evidence::probe::{Baseline, Probe, Verdict};
pub use crate::evidence::survey::{Finding, Survey, contradictions};
