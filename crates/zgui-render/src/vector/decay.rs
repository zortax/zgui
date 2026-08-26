//! When a scratch texture is allowed to become smaller again.
//!
//! A scratch that only ever grows is a high-water mark for the life of the window: one frame that
//! needed a large one holds the memory until the process ends, and nothing anywhere reads that the
//! frame is long over. Growing is still immediate — a frame is not made to wait for room it needs —
//! but shrinking waits, because a scroll or a resize is a run of frames whose demand swings and
//! reallocating a texture in the middle of one costs more than the memory is worth.

use std::time::{Duration, Instant};

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

    /// The granularity width and height are asked for at.
    ///
    /// The class every other resize-sensitive texture in the workspace rounds to. A drag whose
    /// regions grow a few pixels per frame otherwise reallocates the texture on every step; per
    /// class, it reallocates once per 256 pixels crossed, at a bounded cost in texels held.
    pub const SIZE_CLASS: u32 = 256;

    /// This extent with its width and height rounded up to the class, layers untouched.
    #[must_use]
    pub fn classed(self) -> Self {
        let up = |texels: u32| texels.max(1).div_ceil(Self::SIZE_CLASS) * Self::SIZE_CLASS;
        Self {
            width: up(self.width),
            height: up(self.height),
            layers: self.layers,
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
    /// When the current below-high-water window began.
    quiet_since: Option<Instant>,
}

impl Decay {
    /// How long demand must stay below the high-water allocation before it shrinks.
    pub const GRACE: Duration = Duration::from_secs(2);

    /// A decay for a scratch that has nothing allocated.
    pub fn new() -> Self {
        Self::default()
    }

    /// What is allocated now.
    pub fn held(&self) -> Extent {
        self.held
    }

    /// Forgets an allocation released by explicit idle maintenance.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// What the texture should become for a frame asking for `want`, or `None` to keep what is held.
    ///
    /// Growth is immediate and is a union rather than a replacement, so a frame that is wider and
    /// shorter than the last one does not lose the height. Shrinking is the window's own maximum,
    /// not this frame's, which is what keeps a fling from reallocating on the one quiet frame
    /// between two busy ones.
    pub fn wants(&mut self, want: Extent) -> Option<Extent> {
        self.wants_at(want, Instant::now())
    }

    /// Deterministic form of [`Decay::wants`] for callers that already hold a clock reading.
    pub fn wants_at(&mut self, want: Extent, now: Instant) -> Option<Extent> {
        if !self.held.covers(want) || self.held == Extent::NONE {
            self.held = self.held.union(want);
            // The frame that grew the texture is the reason it is this size, so the wait starts
            // after it and not with it. Counting it would make every shrink take two waits: the
            // first window would find its own maximum equal to what is held and conclude nothing.
            self.recent = Extent::NONE;
            self.quiet_since = Some(now);
            return Some(self.held);
        }
        self.recent = self.recent.union(want);
        if self.recent == self.held {
            self.recent = Extent::NONE;
            self.quiet_since = Some(now);
            return None;
        }
        let since = self.quiet_since.get_or_insert(now);
        if now.saturating_duration_since(*since) < Self::GRACE {
            return None;
        }
        let settled = self.recent;
        self.recent = Extent::NONE;
        self.quiet_since = Some(now);
        if settled == self.held {
            return None;
        }
        self.held = settled;
        Some(settled)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Decay, Extent};

    fn later(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn the_first_frame_allocates_exactly_what_it_asked_for() {
        let mut decay = Decay::new();
        let now = Instant::now();
        let want = Extent::new(640, 480, 4);
        assert_eq!(decay.wants_at(want, now), Some(want));
        assert_eq!(decay.held(), want);
    }

    #[test]
    fn a_frame_that_needs_more_gets_it_at_once() {
        let mut decay = Decay::new();
        let now = Instant::now();
        decay.wants_at(Extent::new(640, 480, 4), now);
        assert_eq!(
            decay.wants_at(Extent::new(640, 480, 9), now),
            Some(Extent::new(640, 480, 9))
        );
    }

    #[test]
    fn growth_is_a_union_so_a_wider_shorter_frame_keeps_the_height() {
        let mut decay = Decay::new();
        let now = Instant::now();
        decay.wants_at(Extent::new(200, 800, 4), now);
        assert_eq!(
            decay.wants_at(Extent::new(900, 100, 4), now),
            Some(Extent::new(900, 800, 4))
        );
    }

    #[test]
    fn the_scratch_shrinks_when_the_content_does() {
        let mut decay = Decay::new();
        let now = Instant::now();
        decay.wants_at(Extent::new(1344, 896, 40), now);
        let small = Extent::new(640, 480, 2);
        assert_eq!(
            decay.wants_at(small, later(now, 1_999)),
            None,
            "shrinking before the wait is over would reallocate inside every scroll"
        );
        assert_eq!(decay.wants_at(small, later(now, 2_000)), Some(small));
        assert_eq!(decay.held(), small);
    }

    #[test]
    fn the_scratch_does_not_shrink_inside_a_fling() {
        let mut decay = Decay::new();
        let now = Instant::now();
        let large = Extent::new(1344, 896, 40);
        let small = Extent::new(640, 480, 2);
        decay.wants_at(large, now);
        // A scroll's demand swings: most frames are cheap and every so often one is not. Six hundred
        // frames of that must not reallocate once, because each reallocation throws away a texture
        // the very next busy frame asks for again.
        for frame in 0..600_u64 {
            let want = if frame % 30 == 0 { large } else { small };
            assert_eq!(
                decay.wants_at(want, later(now, frame * 16)),
                None,
                "the scratch was reallocated at frame {frame} of a fling"
            );
        }
        assert_eq!(decay.held(), large);
    }

    #[test]
    fn a_settled_window_shrinks_once_and_then_stays() {
        let mut decay = Decay::new();
        let now = Instant::now();
        decay.wants_at(Extent::new(1344, 896, 40), now);
        let small = Extent::new(640, 480, 2);
        assert_eq!(decay.wants_at(small, later(now, 2_000)), Some(small));
        assert_eq!(
            decay.wants_at(small, later(now, 10_000)),
            None,
            "a texture already the right size must not be reallocated for being the right size"
        );
    }

    #[test]
    fn a_classed_drag_reallocates_per_class_crossed_and_never_per_step() {
        let mut decay = Decay::new();
        let now = Instant::now();
        let mut allocations = 0;
        for step in 0..100u32 {
            let want = Extent::new(800 + step * 8, 600, 4).classed();
            if decay
                .wants_at(want, later(now, u64::from(step) * 16))
                .is_some()
            {
                allocations += 1;
            }
        }
        // 800 through 1592 texels crosses the classes at 1024, 1280 and 1536, plus the first
        // allocation itself.
        assert_eq!(
            allocations, 4,
            "a 100-step grow-drag reallocated {allocations} times"
        );
    }

    #[test]
    fn a_classed_extent_rounds_up_and_keeps_its_layers() {
        assert_eq!(Extent::new(1, 1, 7).classed(), Extent::new(256, 256, 7));
        assert_eq!(Extent::new(256, 512, 3).classed(), Extent::new(256, 512, 3));
        assert_eq!(Extent::new(257, 511, 3).classed(), Extent::new(512, 512, 3));
        assert_eq!(Extent::new(0, 0, 1).classed(), Extent::new(256, 256, 1));
    }
}
