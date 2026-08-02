//! What a drop on a given target would do.

/// What dropping here means.
///
/// The effect is decided by whatever owns the target, per drag, because the same list may accept a
/// move from itself and a copy from elsewhere. [`DropEffect::None`] is a refusal, and it is the
/// default: a target says what it accepts rather than being assumed to accept everything.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DropEffect {
    /// Nothing: this target refuses this drag.
    #[default]
    None,
    /// The dragged thing moves here and leaves where it was.
    Move,
    /// A copy of it appears here and the original stays.
    Copy,
    /// A reference to it appears here.
    Link,
}

impl DropEffect {
    /// Whether a drop would be accepted at all.
    pub const fn accepts(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[cfg(test)]
mod tests {
    use super::DropEffect;

    #[test]
    fn a_target_refuses_until_it_says_otherwise() {
        assert_eq!(DropEffect::default(), DropEffect::None);
        assert!(!DropEffect::default().accepts());
    }

    #[test]
    fn every_other_effect_accepts() {
        for effect in [DropEffect::Move, DropEffect::Copy, DropEffect::Link] {
            assert!(effect.accepts());
        }
    }
}
