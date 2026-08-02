//! When a scratch texture is allowed to become smaller again.
//!
//! A scratch that only ever grows is a high-water mark for the life of the window: one frame that
//! needed a large one holds the memory until the process ends, and nothing anywhere reads that the
//! frame is long over. Growing is still immediate — a frame is not made to wait for room it needs —
//! but shrinking waits, because a scroll or a resize is a run of frames whose demand swings and
//! reallocating a texture in the middle of one costs more than the memory is worth.

/// What one scratch texture is asked to be.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extent {
    /// Texels across.
    pub width: u32,
    /// Texels down.
    pub height: u32,
    /// Layers.
    pub layers: u32,
}

impl Extent {
    /// Nothing allocated at all.
    pub const NONE: Self = Self {
        width: 0,
        height: 0,
        layers: 0,
    };

    /// An extent of `width` by `height` in `layers` layers.
    pub fn new(width: u32, height: u32, layers: u32) -> Self {
        Self {
            width,
            height,
            layers,
        }
    }

    /// The smallest extent containing both.
    pub fn union(self, other: Self) -> Self {
        Self {
            width: self.width.max(other.width),
            height: self.height.max(other.height),
            layers: self.layers.max(other.layers),
        }
    }

    /// Whether every dimension of this one is at least the other's.
    pub fn covers(self, other: Self) -> bool {
        self.width >= other.width && self.height >= other.height && self.layers >= other.layers
    }
}

/// How much a scratch is holding, and how long it has been holding more than it needs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Decay {
    /// What is allocated now.
    held: Extent,
    /// The most any frame of the current window has asked for.
    recent: Extent,
    /// How many frames of the current window have gone by.
    waited: u32,
}

impl Decay {
    /// How many consecutive frames must ask for less before the texture is reallocated smaller.
    ///
    /// Two seconds at sixty frames a second. The number that matters is not how much memory is held
    /// a moment longer than necessary but how a fling behaves: a scroll's demand swings frame to
    /// frame, and the window's *maximum* is what a shrink is measured against, so a run that keeps
    /// reaching the same size keeps it however long the run lasts.
    pub const PATIENCE: u32 = 120;

    /// A decay for a scratch that has nothing allocated.
    pub fn new() -> Self {
        Self::default()
    }

    /// What is allocated now.
    pub fn held(&self) -> Extent {
        self.held
    }

    /// What the texture should become for a frame asking for `want`, or `None` to keep what is held.
    ///
    /// Growth is immediate and is a union rather than a replacement, so a frame that is wider and
    /// shorter than the last one does not lose the height. Shrinking is the window's own maximum,
    /// not this frame's, which is what keeps a fling from reallocating on the one quiet frame
    /// between two busy ones.
    pub fn wants(&mut self, want: Extent) -> Option<Extent> {
        if !self.held.covers(want) || self.held == Extent::NONE {
            self.held = self.held.union(want);
            // The frame that grew the texture is the reason it is this size, so the wait starts
            // after it and not with it. Counting it would make every shrink take two waits: the
            // first window would find its own maximum equal to what is held and conclude nothing.
            self.recent = Extent::NONE;
            self.waited = 0;
            return Some(self.held);
        }
        self.recent = self.recent.union(want);
        self.waited += 1;
        if self.waited < Self::PATIENCE {
            return None;
        }
        let settled = self.recent;
        self.recent = want;
        self.waited = 0;
        if settled == self.held {
            return None;
        }
        self.held = settled;
        Some(settled)
    }
}

#[cfg(test)]
mod tests {
    use super::{Decay, Extent};

    /// Runs `frames` frames of the same demand and answers the last reallocation, if any.
    fn run(decay: &mut Decay, want: Extent, frames: u32) -> Option<Extent> {
        (0..frames).filter_map(|_| decay.wants(want)).last()
    }

    #[test]
    fn the_first_frame_allocates_exactly_what_it_asked_for() {
        let mut decay = Decay::new();
        let want = Extent::new(640, 480, 4);
        assert_eq!(decay.wants(want), Some(want));
        assert_eq!(decay.held(), want);
    }

    #[test]
    fn a_frame_that_needs_more_gets_it_at_once() {
        let mut decay = Decay::new();
        decay.wants(Extent::new(640, 480, 4));
        assert_eq!(
            decay.wants(Extent::new(640, 480, 9)),
            Some(Extent::new(640, 480, 9))
        );
    }

    #[test]
    fn growth_is_a_union_so_a_wider_shorter_frame_keeps_the_height() {
        let mut decay = Decay::new();
        decay.wants(Extent::new(200, 800, 4));
        assert_eq!(
            decay.wants(Extent::new(900, 100, 4)),
            Some(Extent::new(900, 800, 4))
        );
    }

    #[test]
    fn the_scratch_shrinks_when_the_content_does() {
        let mut decay = Decay::new();
        decay.wants(Extent::new(1344, 896, 40));
        let small = Extent::new(640, 480, 2);
        assert_eq!(
            run(&mut decay, small, Decay::PATIENCE - 1),
            None,
            "shrinking before the wait is over would reallocate inside every scroll"
        );
        assert_eq!(decay.wants(small), Some(small));
        assert_eq!(decay.held(), small);
    }

    #[test]
    fn the_scratch_does_not_shrink_inside_a_fling() {
        let mut decay = Decay::new();
        let large = Extent::new(1344, 896, 40);
        let small = Extent::new(640, 480, 2);
        decay.wants(large);
        // A scroll's demand swings: most frames are cheap and every so often one is not. Six hundred
        // frames of that must not reallocate once, because each reallocation throws away a texture
        // the very next busy frame asks for again.
        for frame in 0..600 {
            let want = if frame % 30 == 0 { large } else { small };
            assert_eq!(
                decay.wants(want),
                None,
                "the scratch was reallocated at frame {frame} of a fling"
            );
        }
        assert_eq!(decay.held(), large);
    }

    #[test]
    fn a_settled_window_shrinks_once_and_then_stays() {
        let mut decay = Decay::new();
        decay.wants(Extent::new(1344, 896, 40));
        let small = Extent::new(640, 480, 2);
        assert_eq!(run(&mut decay, small, Decay::PATIENCE), Some(small));
        assert_eq!(
            run(&mut decay, small, Decay::PATIENCE * 4),
            None,
            "a texture already the right size must not be reallocated for being the right size"
        );
    }
}
