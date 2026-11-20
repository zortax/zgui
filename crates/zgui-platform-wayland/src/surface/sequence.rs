//! Which configures belong to the mapping a surface has now.
//!
//! A commit with no buffer unmaps a mapped surface, and the shell then forgets the configures it
//! sent for that mapping. A wlroots shell treats a later acknowledgement of one of them as a
//! protocol error. Configures sent before the unmap can still arrive after it, because the shell
//! sends them while the unmap is on its way. So the unmap sends a sync right after the commit:
//! the shell answers it in order, and every configure that arrives before the answer was sent
//! before the unmap. Those are dropped unacknowledged.
//!
//! After an unmap the shell holds no role state for the surface. The next mapping starts with the
//! initial commit again, and only configures that answer that commit belong to it.

use zgui_platform::SurfaceId;

/// What a sync sent after an unmap answers for: the surface, and which unmap it followed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Barrier {
    /// The surface that was unmapped.
    pub(crate) surface: SurfaceId,
    /// The number [`Sequence::reset`] gave it.
    pub(crate) serial: u64,
}

/// Where the shell is in the configure sequence of one surface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Sequence {
    /// Whether a commit with no buffer reset the role and no initial commit has followed it.
    reset: bool,
    /// The barrier of the last unmap, while the shell has not answered it.
    barrier: Option<u64>,
    /// The number of the last barrier.
    serial: u64,
}

impl Sequence {
    /// Records a commit with no buffer on a mapped surface, and numbers its barrier.
    pub(crate) const fn reset(&mut self) -> u64 {
        self.serial += 1;
        self.reset = true;
        self.barrier = Some(self.serial);
        self.serial
    }

    /// Records the initial commit of a new mapping.
    pub(crate) const fn restarted(&mut self) {
        self.reset = false;
    }

    /// Records the answer to a barrier. An answer to an older barrier changes nothing.
    pub(crate) fn answered(&mut self, serial: u64) {
        if self.barrier == Some(serial) {
            self.barrier = None;
        }
    }

    /// Whether the next mapping still needs its initial commit.
    pub(crate) const fn needs_initial_commit(&self) -> bool {
        self.reset
    }

    /// Whether a configure that arrives now belongs to the current mapping.
    pub(crate) const fn accepts(&self) -> bool {
        !self.reset && self.barrier.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::Sequence;

    #[test]
    fn a_fresh_surface_takes_every_configure() {
        let sequence = Sequence::default();
        assert!(sequence.accepts());
        assert!(!sequence.needs_initial_commit());
    }

    #[test]
    fn a_configure_sent_before_the_unmap_is_dropped() {
        let mut sequence = Sequence::default();
        let barrier = sequence.reset();
        assert!(!sequence.accepts());
        assert!(sequence.needs_initial_commit());
        sequence.answered(barrier);
        // The shell holds no role state until the initial commit goes out again.
        assert!(!sequence.accepts());
        sequence.restarted();
        assert!(sequence.accepts());
    }

    #[test]
    fn a_remap_before_the_answer_waits_for_the_answer() {
        let mut sequence = Sequence::default();
        let barrier = sequence.reset();
        sequence.restarted();
        assert!(!sequence.needs_initial_commit());
        assert!(!sequence.accepts(), "a configure from before the unmap");
        sequence.answered(barrier);
        assert!(sequence.accepts(), "the configure that answers the remap");
    }

    #[test]
    fn only_the_last_barrier_opens_the_sequence() {
        let mut sequence = Sequence::default();
        let first = sequence.reset();
        sequence.restarted();
        let second = sequence.reset();
        sequence.restarted();
        sequence.answered(first);
        assert!(!sequence.accepts());
        sequence.answered(second);
        assert!(sequence.accepts());
    }
}
