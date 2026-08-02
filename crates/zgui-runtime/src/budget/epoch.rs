//! The stamp a cache's last use is recorded against.

/// A monotonic frame stamp.
///
/// It is a serial number and not a time: what a budget asks of it is only ever "which of these two
/// was used more recently", and a duration would invite an answer in seconds that no policy here
/// wants. One window advances one of these once per frame it paints, so two windows' stamps are not
/// comparable and nothing here compares them.
///
/// It is deliberately not a version in a concurrency protocol. Nothing here is shared across
/// threads, nothing compares it to decide whether a write was seen, and a consumer that wanted
/// either of those would need a different type rather than a wider use of this one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneEpoch(u64);

impl SceneEpoch {
    /// The stamp a window that has painted nothing is at.
    ///
    /// A cache never used is at this stamp, which is what makes it sort ahead of everything used
    /// since — a cache holding content nothing has ever read is the coldest thing there is.
    pub const FIRST: Self = Self(0);

    /// The stamp after this one.
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// How many frames have passed since [`SceneEpoch::FIRST`].
    pub const fn get(self) -> u64 {
        self.0
    }

    /// How many frames separate this stamp from a later one.
    ///
    /// Saturating, so an argument that is somehow earlier reads as no distance at all rather than
    /// as an enormous one — the only use for the answer is "how cold", and a negative age is not a
    /// state a policy should be made to have an opinion about.
    pub const fn frames_before(self, later: Self) -> u64 {
        later.0.saturating_sub(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::SceneEpoch;

    #[test]
    fn a_cache_never_used_sorts_ahead_of_one_used_on_the_first_frame() {
        assert!(SceneEpoch::FIRST < SceneEpoch::FIRST.next());
    }

    #[test]
    fn distance_to_an_earlier_stamp_is_no_distance_rather_than_an_enormous_one() {
        let later = SceneEpoch::FIRST.next().next();
        assert_eq!(SceneEpoch::FIRST.frames_before(later), 2);
        assert_eq!(later.frames_before(SceneEpoch::FIRST), 0);
    }
}
