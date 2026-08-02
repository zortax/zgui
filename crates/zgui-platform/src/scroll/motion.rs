//! Whether a wheel detent arrives whole or already spread over time.

/// How a notched wheel's detents reach this program.
///
/// A detent is a discrete event — the wheel clicked once — but it is not shown discretely by any
/// application anybody wants to use: the content travels to its new place over a couple of hundred
/// milliseconds so the eye can follow it. The question this answers is *who* does that travelling,
/// and both answers are real.
///
/// Getting it wrong is smooth in one direction and unusable in the other. A framework that animates
/// a platform's already-animated stream animates each of its thirty little deltas separately, and
/// the result crawls; one that does not animate a whole detent jumps the document by a hundred
/// pixels per click.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WheelMotion {
    /// One detent arrives as one whole delta, and nothing has animated it.
    ///
    /// What X11, Wayland and Win32 all deliver: the axis event carries the whole click. Something
    /// above this seam has to carry the content there over several frames, or the document jumps.
    #[default]
    Discrete,
    /// The platform is already spreading each detent over time.
    ///
    /// What macOS does, and what a precision touch surface does everywhere: the events are a
    /// continuous stream of small deltas that already describe a decelerating motion, including the
    /// momentum after the fingers have left. Adding a second animation on top of that one is what
    /// makes a trackpad feel like it is scrolling through treacle.
    Continuous,
}

impl WheelMotion {
    /// Whether the framework is the one that has to animate a detent.
    pub const fn framework_animates(self) -> bool {
        matches!(self, Self::Discrete)
    }
}

#[cfg(test)]
mod tests {
    use super::WheelMotion;

    #[test]
    fn a_whole_detent_is_the_frameworks_to_animate_and_a_stream_is_not() {
        assert!(WheelMotion::Discrete.framework_animates());
        assert!(!WheelMotion::Continuous.framework_animates());
        assert_eq!(WheelMotion::default(), WheelMotion::Discrete);
    }
}
