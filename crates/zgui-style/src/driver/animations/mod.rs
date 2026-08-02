//! The half of the animation tick that has to be inside the style engine.
//!
//! Transitions and keyframe animations are the engine's own machinery: it decides when one starts
//! from the difference between two cascade results, it holds the interpolation for every animatable
//! property, and it knows what fill modes, iteration counts and directions mean. None of that is
//! worth reimplementing, and none of it can be reached from outside this crate, so the mechanical
//! part of the tick lives here — advancing the clock, moving each running animation through its
//! states, and sampling what every animating element's values currently are.
//!
//! What this module deliberately does **not** do is decide anything. It reports; the decisions —
//! which elements take the cheap repaint-only path, which have to go back through the cascade, what
//! is written where, and when the loop must wake up again — belong to the caller.
//!
//! | Module | Contents |
//! |---|---|
//! | [`set`] | the animations a document is running, and the clock they are read at |
//! | [`tick`] | advancing them one frame, and the lifecycle edges that produces |
//! | [`sample`] | what one element's animations currently evaluate to |

pub mod sample;
pub mod set;
pub mod tick;

pub use crate::driver::animations::sample::AnimatedProperties;
pub use crate::driver::animations::set::Animations;
pub use crate::driver::animations::tick::{
    AnimationEdge, AnimationReport, ElementAnimation, Lifecycle, TimedKind,
};

/// The time the cascade resolves animation-derived values at.
///
/// Measured from the start of the document rather than from the epoch, and read from the frame
/// clock rather than from the wall clock, so that a test advancing a virtual clock advances this
/// with it.
#[derive(Copy, Clone, PartialEq, PartialOrd, Debug, Default)]
pub struct AnimationTime(pub f64);

impl AnimationTime {
    /// The time a document starts at.
    pub const START: Self = Self(0.0);

    /// The value the engine's context wants.
    pub(crate) fn seconds(self) -> f64 {
        self.0
    }
}
