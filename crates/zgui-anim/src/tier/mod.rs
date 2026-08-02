//! Which path an element's animations take, decided once from what they move.
//!
//! The decision is a mask test and nothing else, deliberately. It has to be made for every animating
//! element on every frame — five hundred of them on a screen full of loading skeletons — so anything
//! that walked a property list per element would cost more than the work it is choosing between.

use zgui_style::AnimatedProperties;

/// How an element's animations are going to be applied this frame.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Tier {
    /// Compose the interpolated values over the shared lowered style and repaint.
    ///
    /// Available when every property moving is one whose value nothing between the cascade and the
    /// painter reads: no descendant inherits it, no fragment's position or extent depends on it,
    /// and no hit rectangle moves. What the frame owes is then exactly a repaint of a rectangle
    /// that already exists.
    Cheap,
    /// Compose the element's fragments again from an interpolated placement, and repaint what that
    /// moved.
    ///
    /// Available when the only thing moving besides paint is *where* the box is drawn — a
    /// `transform`, a `translate`, a `rotate`, a `scale`, or the origin they turn about. A
    /// transform is not a repaint: it moves the box's ink, the rectangle a hit test answers over,
    /// and the device-space position of everything below it. But all three of those are composed by
    /// the fragment pass out of one matrix, and that matrix is the only thing that has to change —
    /// no size is computed again, no box is rebuilt, and the shared style a thousand identical
    /// elements are all drawn from is not touched.
    ///
    /// What this tier may *not* express is a transform coming into existence or going out of it,
    /// because that changes whether the element establishes a stacking context and whether it is
    /// the containing block for the positioned boxes below it. Both are read from the shared style,
    /// so both need the cascade; the sampler recognises the case and reports the element as
    /// cascading rather than placing.
    Place,
    /// Run the element's cascade again with its animation declarations replaced.
    ///
    /// Correct for every animatable property there is, and the only thing that is: a filter changes
    /// how far a fragment's pixels reach, a length moves every box around it, and an inherited
    /// value is computed from by every descendant. Each of those creates an obligation that neither
    /// of the cheaper paths can discharge.
    Cascade,
}

impl Tier {
    /// The path an element moving `properties` takes.
    ///
    /// ```
    /// use zgui_anim::Tier;
    /// use zgui_style::AnimatedProperties;
    ///
    /// let fade = AnimatedProperties::OPACITY | AnimatedProperties::PAINT_COLOR;
    /// assert_eq!(Tier::of(fade), Tier::Cheap);
    /// assert_eq!(Tier::of(fade | AnimatedProperties::TRANSFORM), Tier::Place);
    /// assert_eq!(Tier::of(fade | AnimatedProperties::CASCADED), Tier::Cascade);
    /// ```
    ///
    /// An element moving *nothing* takes the general path, which costs nothing because there is
    /// nothing to apply — the alternative would be a "cheap" answer for an element with no
    /// animation, which is the shape a vacuous count is made of.
    pub fn of(properties: AnimatedProperties) -> Self {
        if properties.is_paint_only() {
            Self::Cheap
        } else if properties.is_placement_only() {
            Self::Place
        } else {
            Self::Cascade
        }
    }

    /// Whether this is the repaint-only path.
    pub fn is_cheap(self) -> bool {
        matches!(self, Self::Cheap)
    }

    /// Whether this path leaves the element's own cascade result standing.
    ///
    /// True for both of the cheaper paths, which is the question the frame asks: an element whose
    /// values are written into its own columns is one the restyle must not be asked about, and one
    /// whose overrides have to be dropped the moment its animation lets go of it.
    pub fn is_overriding(self) -> bool {
        matches!(self, Self::Cheap | Self::Place)
    }
}

#[cfg(test)]
mod tests {
    use zgui_style::AnimatedProperties;

    use super::Tier;

    #[test]
    fn a_fade_is_cheap_and_a_length_is_not() {
        assert_eq!(Tier::of(AnimatedProperties::OPACITY), Tier::Cheap);
        assert_eq!(Tier::of(AnimatedProperties::PAINT_COLOR), Tier::Cheap);
        assert_eq!(Tier::of(AnimatedProperties::CASCADED), Tier::Cascade);
    }

    #[test]
    fn a_transform_is_placed_rather_than_cascaded() {
        // The whole point of the middle tier: a progress bar sliding under its own clip re-places
        // one fragment, and used to re-cascade one element on every frame it produced, for ever.
        assert_eq!(Tier::of(AnimatedProperties::TRANSFORM), Tier::Place);
        assert!(Tier::of(AnimatedProperties::TRANSFORM).is_overriding());
        assert!(!Tier::of(AnimatedProperties::TRANSFORM).is_cheap());
    }

    #[test]
    fn a_transform_that_also_fades_is_still_placed() {
        // The two overrides go to two different places and are read by two different stages, so an
        // element doing both needs neither the cascade nor a choice between them.
        let both = AnimatedProperties::TRANSFORM | AnimatedProperties::OPACITY;
        assert_eq!(Tier::of(both), Tier::Place);
    }

    #[test]
    fn one_property_outside_the_set_takes_the_whole_element_with_it() {
        // The rule is a union, not a majority: an element fading *and* moving a width still has to
        // cascade, because the width is what neither override can express.
        let mixed = AnimatedProperties::OPACITY
            | AnimatedProperties::PAINT_COLOR
            | AnimatedProperties::TRANSFORM
            | AnimatedProperties::CASCADED;
        assert_eq!(Tier::of(mixed), Tier::Cascade);
    }

    #[test]
    fn nothing_moving_is_not_a_cheap_animation() {
        assert_eq!(Tier::of(AnimatedProperties::empty()), Tier::Cascade);
        assert!(!Tier::of(AnimatedProperties::empty()).is_cheap());
        assert!(!Tier::of(AnimatedProperties::empty()).is_overriding());
    }
}
