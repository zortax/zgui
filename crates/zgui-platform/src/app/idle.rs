//! What the loop should do when it has nothing left to do.

use std::time::Instant;

/// What the loop should do before it blocks.
///
/// The default is to block until something happens, and that is the whole design of the frame
/// loop: a user interface that is not changing should consume no processor time at all. The two
/// other answers exist for the cases where that is genuinely wrong.
///
/// [`IdlePolicy::BlockUntil`] parks with a deadline, and is how a timer or an animation gets a
/// frame without anything spinning in between. The deadline must be **strictly in the future**: a
/// deadline that has already passed is reported as expired on every turn of the loop, forever,
/// which turns a parked loop into a busy one running no frames at all. When a deadline has been
/// reached the answer is to ask the relevant surfaces to redraw and then park with
/// [`IdlePolicy::Block`].
///
/// ```
/// use std::time::{Duration, Instant};
/// use zgui_platform::IdlePolicy;
///
/// let now = Instant::now();
/// let already_passed = now - Duration::from_millis(1);
/// assert_eq!(IdlePolicy::until(already_passed, now), IdlePolicy::Block);
///
/// let ahead = now + Duration::from_millis(16);
/// assert_eq!(IdlePolicy::until(ahead, now), IdlePolicy::BlockUntil(ahead));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdlePolicy {
    /// Sleep until the platform has something to report.
    #[default]
    Block,
    /// Sleep until the platform has something to report, or until this moment arrives.
    BlockUntil(Instant),
    /// Do not sleep at all.
    ///
    /// This runs the loop as fast as the machine allows and is for measurement and for a
    /// presentation mode that is paced by the display instead. Ordinary use never asks for it.
    Spin,
}

impl IdlePolicy {
    /// The policy for parking until `deadline`, given that it is now `now`.
    ///
    /// A deadline that is not strictly in the future collapses to [`IdlePolicy::Block`], because
    /// installing it would produce a loop that wakes instantly and repeats for ever.
    pub fn until(deadline: Instant, now: Instant) -> Self {
        if deadline > now {
            Self::BlockUntil(deadline)
        } else {
            Self::Block
        }
    }

    /// The earlier of two policies, so several deadlines merge into the one that comes first.
    ///
    /// Spinning wins over everything, because something asked for every frame there is.
    pub fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Spin, _) | (_, Self::Spin) => Self::Spin,
            (Self::BlockUntil(a), Self::BlockUntil(b)) => Self::BlockUntil(a.min(b)),
            (Self::BlockUntil(deadline), Self::Block)
            | (Self::Block, Self::BlockUntil(deadline)) => Self::BlockUntil(deadline),
            (Self::Block, Self::Block) => Self::Block,
        }
    }

    /// The moment this policy parks until, when it parks with a deadline at all.
    pub const fn deadline(self) -> Option<Instant> {
        match self {
            Self::BlockUntil(deadline) => Some(deadline),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IdlePolicy;
    use std::time::{Duration, Instant};

    #[test]
    fn a_deadline_in_the_past_is_never_installed() {
        let now = Instant::now();
        assert_eq!(IdlePolicy::until(now, now), IdlePolicy::Block);
        assert_eq!(
            IdlePolicy::until(now - Duration::from_secs(1), now),
            IdlePolicy::Block
        );
        assert_eq!(IdlePolicy::until(now, now).deadline(), None);
    }

    #[test]
    fn merging_deadlines_keeps_the_earliest() {
        let now = Instant::now();
        let soon = now + Duration::from_millis(8);
        let later = now + Duration::from_millis(700);
        assert_eq!(
            IdlePolicy::BlockUntil(later).merge(IdlePolicy::BlockUntil(soon)),
            IdlePolicy::BlockUntil(soon)
        );
        assert_eq!(
            IdlePolicy::Block.merge(IdlePolicy::BlockUntil(soon)),
            IdlePolicy::BlockUntil(soon)
        );
        assert_eq!(
            IdlePolicy::Block.merge(IdlePolicy::Block),
            IdlePolicy::Block
        );
    }

    #[test]
    fn spinning_outranks_every_deadline() {
        let ahead = Instant::now() + Duration::from_millis(1);
        assert_eq!(
            IdlePolicy::BlockUntil(ahead).merge(IdlePolicy::Spin),
            IdlePolicy::Spin
        );
        assert_eq!(IdlePolicy::Spin.merge(IdlePolicy::Block), IdlePolicy::Spin);
    }

    #[test]
    fn blocking_is_what_a_loop_does_by_default() {
        assert_eq!(IdlePolicy::default(), IdlePolicy::Block);
    }
}
