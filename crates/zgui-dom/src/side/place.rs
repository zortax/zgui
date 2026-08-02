//! Where a box is drawn, when an animation and not the cascade is deciding it.
//!
//! A transform is not a colour. It changes no pixel of the box's own paint description — the fill,
//! the border and the shadow are all exactly what they were — and it changes everything about
//! *where* those pixels land: the box's ink rectangle, the rectangle a hit test answers over, and
//! the device-space position of every descendant. So it cannot be composed at the moment of
//! emission the way [`AnimOverride`](crate::side::AnimOverride) is; it has to be in hand while the
//! fragment is composed, which is the one stage that turns a style into geometry.
//!
//! This record is what a fragment pass is given in place of the box's shared style for that one
//! question. Everything else about the box — what establishes a stacking context, what is a
//! containing block for a fixed descendant, whether the subtree composites as a group — is read
//! from the shared style and must go on agreeing with it, which is why a transform animation that
//! would change any of those answers is refused this path and sent back through the cascade.

use servo_arc::Arc as ServoArc;
use style::properties::style_structs;

/// The interpolated placement one animation is currently imposing on one element.
///
/// Held as the whole `Box` property group rather than as the four transform properties on their
/// own, because the matrix is resolved from all four together with `transform-origin`, and a copy
/// of five values that has to be kept in step with a group that already holds them is a second
/// place for them to disagree. The group is shared, so carrying it costs a pointer.
#[derive(Clone, Debug)]
pub struct AnimPlacement {
    /// The animated `Box` group, as the style engine produced it.
    group: ServoArc<style_structs::Box>,
}

impl AnimPlacement {
    /// The placement `group` describes.
    pub fn new(group: ServoArc<style_structs::Box>) -> Self {
        Self { group }
    }

    /// The property group a fragment's transform is resolved from.
    pub fn group(&self) -> &style_structs::Box {
        &self.group
    }
}

impl PartialEq for AnimPlacement {
    /// Whether two placements put the box in the same place.
    ///
    /// Compared by value and not by allocation. Applying an animation to a cascade result copies
    /// the group it writes into, so every frame of every animation produces a fresh allocation and
    /// an identity test would report a change on a value that has stopped moving — which is a
    /// fragment recomposed, an ink rectangle damaged and a hit entry rewritten on every frame the
    /// window runs, for ever, over a transform that is holding still.
    fn eq(&self, other: &Self) -> bool {
        ServoArc::ptr_eq(&self.group, &other.group) || self.group == other.group
    }
}

#[cfg(test)]
mod tests {
    use super::AnimPlacement;

    #[test]
    fn a_placement_costs_one_pointer() {
        assert_eq!(size_of::<AnimPlacement>(), size_of::<usize>());
    }
}
