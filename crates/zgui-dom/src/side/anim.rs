//! Where a cheap animation writes, so that it does not write anywhere shared.
//!
//! Identically styled elements share their lowered paint description by construction — that sharing
//! is what makes a thousand-row list cheap to paint. An animation that wrote an interpolated value
//! into the shared description would animate every element that shares it, so eight identical
//! buttons would all light up when one is hovered. The bug is invisible in a fixture with one
//! button and obvious in a real interface, which is why the private place to write exists before
//! anything writes to it.
//!
//! This column is that place: live only while a cheap-path animation is running on the node,
//! absent for everything else, and composed over the shared description at the moment of emission.
//!
//! # Why only these properties
//!
//! Everything recorded here changes what a box is *painted in* and nothing about where it, its
//! descendants or its hit rectangle are. That is what makes the cheap path cheap: the obligation an
//! interpolated value creates is a repaint of the rectangle the box already occupies, and no stage
//! between the animation and the painter has to run.
//!
//! A property that moves a fragment creates a second obligation that only the fragment pass can
//! discharge, so it is not here. Where it goes instead depends on what else it moves: a transform
//! goes to [`AnimPlacement`](crate::side::AnimPlacement), which the fragment pass reads while it
//! composes the box; a filter, whose kernel grows the ink the fragment reads and writes, goes back
//! through the cascade.

use style::color::AbsoluteColor;

/// The values a cheap-path animation is currently overriding on one node.
///
/// Each field is `None` unless an animation is driving it, so composing an override is a handful of
/// tests against nothing in the common case.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimOverride {
    /// The animated `opacity`.
    pub opacity: Option<f32>,
    /// The animated `background-color`.
    pub background_color: Option<AbsoluteColor>,
    /// The animated border colours: top, right, bottom and left, in that order.
    pub border_colors: Option<[AbsoluteColor; 4]>,
    /// The animated `outline-color`.
    pub outline_color: Option<AbsoluteColor>,
}

impl AnimOverride {
    /// An override that changes nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether this override changes nothing, in which case the node can be dropped from the
    /// column entirely.
    pub fn is_empty(&self) -> bool {
        self.opacity.is_none()
            && self.background_color.is_none()
            && self.border_colors.is_none()
            && self.outline_color.is_none()
    }

    /// A number that changes whenever any overridden value does.
    ///
    /// A consumer that caches what it drew for a fragment compares this against the number the
    /// cached drawing was made under. Without it the cache sees one unchanging shared style and
    /// replays the first frame of the animation for the whole of its length — the animation runs,
    /// marks its obligations, and nothing on the screen moves.
    pub fn signature(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let mut mix = |bits: u64| {
            hash ^= bits;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        };
        mix(self
            .opacity
            .map_or(0, |value| value.to_bits() as u64 | 1 << 32));
        for color in self
            .background_color
            .iter()
            .chain(self.outline_color.iter())
            .chain(self.border_colors.iter().flatten())
        {
            for component in color.raw_components() {
                mix(component.to_bits() as u64);
            }
            mix(color.color_space as u64);
        }
        hash
    }
}

/// One node's override, boxed so the column costs a pointer for the nodes that are not animating.
pub type AnimSlot = Option<Box<AnimOverride>>;

#[cfg(test)]
mod tests {
    use super::{AnimOverride, AnimSlot};

    #[test]
    fn a_fresh_override_changes_nothing() {
        assert!(AnimOverride::new().is_empty());
    }

    #[test]
    fn an_overridden_value_makes_the_record_worth_keeping() {
        let mut override_ = AnimOverride::new();
        override_.opacity = Some(0.5);
        assert!(!override_.is_empty());
    }

    #[test]
    fn a_node_that_is_not_animating_costs_one_pointer() {
        assert_eq!(size_of::<AnimSlot>(), size_of::<usize>());
    }

    #[test]
    fn two_frames_of_one_animation_have_different_signatures() {
        // The property a replay cache depends on: the shared lowered style is identical across the
        // whole animation, so if this number did not move the cache would replay frame one for
        // ever.
        let at = |opacity| AnimOverride {
            opacity: Some(opacity),
            ..AnimOverride::new()
        };
        assert_ne!(at(0.5).signature(), at(0.75).signature());
        assert_eq!(at(0.5).signature(), at(0.5).signature());
        assert_ne!(at(0.5).signature(), AnimOverride::new().signature());
    }
}
