//! Whether content dragged past its end follows the gesture or stops dead.

/// When a container may be displaced past its own end.
///
/// Content pulled beyond its last row can either stop at the edge or follow the pull with
/// diminishing returns and spring back when the pull ends. Both are right, for different inputs,
/// and which one is wanted is decided by what the person is scrolling *with* rather than by taste.
///
/// A pointing device with a notched wheel has no gesture to follow. Each detent is a separate,
/// whole instruction that arrives after the last one finished, so there is nothing continuous for
/// the edge to track: what a spring does there is bounce once per click, against an edge the person
/// has already reached and is now simply pushing at. A precision surface is the opposite case — the
/// contact is still down, the displacement past the end *is* the gesture's remaining travel, and an
/// edge that stopped dead would leave the content lagging a finger that is still moving.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Elastic {
    /// Only where the input is a continuous gesture: a touch contact, or a precision surface.
    ///
    /// The default, and what a desktop expects. A notched wheel gets an edge that stops.
    #[default]
    Kinetic,
    /// Wherever a container is pushed past its end, including by a wheel.
    Always,
    /// Nowhere. Every container stops at its own end.
    Never,
}

impl Elastic {
    /// Whether a scroll that arrived whole — one detent of a notched wheel — may displace an edge.
    pub const fn admits_a_detent(self) -> bool {
        matches!(self, Self::Always)
    }

    /// Whether a scroll that is part of a continuous gesture may displace an edge.
    pub const fn admits_a_gesture(self) -> bool {
        matches!(self, Self::Kinetic | Self::Always)
    }
}

#[cfg(test)]
mod tests {
    use super::Elastic;

    #[test]
    fn a_wheel_gets_no_spring_by_default_and_a_gesture_does() {
        let settled = Elastic::default();
        assert_eq!(settled, Elastic::Kinetic);
        assert!(!settled.admits_a_detent());
        assert!(settled.admits_a_gesture());
    }

    #[test]
    fn each_answer_says_the_same_thing_about_both_inputs_or_says_why_not() {
        // The two extremes are the ones with nothing to decide: `Always` admits everything and
        // `Never` admits nothing. Only the default distinguishes, and the distinction is the point.
        assert!(Elastic::Always.admits_a_detent() && Elastic::Always.admits_a_gesture());
        assert!(!Elastic::Never.admits_a_detent() && !Elastic::Never.admits_a_gesture());
    }
}
