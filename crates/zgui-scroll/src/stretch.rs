//! Whether one scroll is allowed to pull an edge past its end.

/// What a scroll may do with the part of itself no container could absorb.
///
/// The leftover of a scroll that reached the end of the outermost container is either a
/// displacement — the edge follows the pull and springs back — or nothing at all. Which of the two
/// is a property of *the scroll*, not of the container: the same list stretches under a finger that
/// is still moving and stops dead under a wheel that has clicked once more, and both are what the
/// person expects from the thing they are holding.
///
/// This is deliberately narrower than the desktop's own preference, which is the platform layer's
/// `Elastic` and says which *inputs* may stretch. By the time a
/// scroll reaches a container the input is known, so the question has already been answered and
/// what travels here is the answer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Stretch {
    /// The leftover displaces the outermost container and springs back.
    #[default]
    Permitted,
    /// The leftover is dropped and the container stops at its end.
    Refused,
}

impl Stretch {
    /// Whether an edge may be displaced.
    pub const fn is_permitted(self) -> bool {
        matches!(self, Self::Permitted)
    }
}

#[cfg(test)]
mod tests {
    use super::Stretch;

    #[test]
    fn a_scroll_that_says_nothing_stretches() {
        // The default is the permissive one because every caller that predates this decision was
        // stretching, and a default that silently stopped them would change what they do without
        // anybody editing them.
        assert!(Stretch::default().is_permitted());
        assert!(!Stretch::Refused.is_permitted());
    }
}
