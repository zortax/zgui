//! What a harness is built from, and the three rules a budget fixture obeys.
//!
//! Each rule below was forced by a running program, and each one exists because a fixture that
//! breaks it produces a test that measures something other than what it names — usually while
//! passing.
//!
//! # 1. A walk-budget fixture uses at most four scattered dirty siblings, or a contiguous run
//!
//! The dirty-children structure holds four exact children and degrades to an inclusive span on the
//! fifth. Measured over ten thousand children, four scattered marks cost **4** probes and five cost
//! **9 997**. A budget written over five scattered dirty rows therefore measures the span rather
//! than the design, and would either fail for the wrong reason or be "fixed" by raising its bound
//! until it asserted nothing. [`walk::SiblingMarks`] refuses to build such a fixture, so the rule is
//! mechanical rather than a paragraph nobody reads. A fixture that genuinely needs more — a
//! reordering test — takes the larger budget deliberately, with the reason written beside it.
//!
//! # 2. A leak assertion counts drops, never arena occupancy
//!
//! The reactive engine exposes no way to read its arena's occupancy: the arena module is private,
//! its types are not re-exported, and there is no length, no iteration and no hook. The only
//! external reading is the debug spelling of a handle, which is fine for a throwaway probe and
//! unacceptable in a shipped assertion. So a leak check counts *drops*: created against dropped over
//! a mount and unmount cycle, which is [`leak::DropLedger`]. **Do not write an "occupancy returned
//! to baseline" assertion; it cannot be written.**
//!
//! # 3. The non-reactive zone is asserted by re-run counts, with a control
//!
//! Nothing about the zone itself is observable — the predicate is private, and there is no counter.
//! The assertable form is behavioural and needs its control: an effect reading a second signal
//! untracked inside the zone re-runs **zero** times when that signal is written, while a tracked
//! control effect reading the same signal re-runs **once**, and the effect still runs once on its
//! own signal. Any criterion that says "enforced by a zone assertion" means that triple, which is
//! [`zone::ZoneEvidence`], and never an inspection of the zone.

pub mod leak;
pub mod walk;
pub mod zone;

use zgui_geom::{Device, Size};

use crate::harness::frame::Pipeline;

/// A document, its surface, and whatever builds a frame from it.
///
/// This is what [`Harness::new`](crate::Harness::new) is given. The frame body is a
/// [`Pipeline`]: a test supplies a closure, and a real engine supplies itself, so the same harness
/// measures both.
pub struct Fixture {
    /// The surface extent frames are built for.
    pub viewport: Size<i32, Device>,
    /// What builds each frame.
    pub pipeline: Box<dyn Pipeline>,
}

impl Fixture {
    /// The default surface a fixture is built for, in device pixels.
    pub const DEFAULT_VIEWPORT: Size<i32, Device> = Size::new(800, 600);

    /// A fixture whose frames are built by `pipeline`, on the default surface.
    pub fn new(pipeline: impl Pipeline + 'static) -> Self {
        Self {
            viewport: Self::DEFAULT_VIEWPORT,
            pipeline: Box::new(pipeline),
        }
    }

    /// The same fixture on a surface of `viewport` device pixels.
    pub fn sized(mut self, viewport: Size<i32, Device>) -> Self {
        self.viewport = viewport;
        self
    }
}
