//! What one element's running animations currently evaluate to, and which properties they move.
//!
//! Sampling is the engine's own interpolation, applied to a copy of the element's cascade result
//! rather than to the result itself. That is what makes it usable outside the cascade: the copy is
//! read for the handful of values a repaint needs and then dropped, so nothing shared by the
//! elements that cascaded to the same result is touched, and no selector is matched.
//!
//! The property set comes back beside the values because it is what the caller classifies on. It is
//! a summary and not a list: the caller's question is only ever which of the frame's stages this
//! element's animations oblige, and answering that from a handful of bits costs a mask test rather
//! than a walk over a property set per element per frame.
//!
//! One question the summary cannot answer, and the sampler therefore answers here: an animation
//! that moves a transform an element already has and one that gives it its first transform are the
//! same property moving, and they are not the same work. Only a comparison of the two styles
//! separates them, and only this module holds both.

use servo_arc::Arc as ServoArc;
use style::animation::ElementAnimationSet;
use style::context::SharedStyleContext;
use style::properties::{ComputedValues, LonghandId, OwnedPropertyDeclarationId};
use style::values::computed::Color as ColorValue;
use zgui_css::values::color::AbsoluteColor;
use zgui_dom::side::{AnimOverride, AnimPlacement};

bitflags::bitflags! {
    /// A summary of which kinds of property an element's animations are moving.
    ///
    /// ```
    /// use zgui_style::driver::animations::AnimatedProperties;
    ///
    /// assert!(AnimatedProperties::OPACITY.is_paint_only());
    /// assert!(!(AnimatedProperties::OPACITY | AnimatedProperties::CASCADED).is_paint_only());
    /// assert!(!AnimatedProperties::empty().is_paint_only());
    /// assert!(AnimatedProperties::TRANSFORM.is_placement_only());
    /// assert!(!AnimatedProperties::TRANSFORM.is_paint_only());
    /// ```
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug)]
    pub struct AnimatedProperties: u8 {
        /// `opacity`, which multiplies the whole subtree's alpha and moves nothing.
        const OPACITY = 1 << 0;
        /// A colour no descendant inherits: a background, a border side, an outline.
        const PAINT_COLOR = 1 << 1;
        /// Anything else at all — a length, a filter, an inherited colour.
        ///
        /// The name is the point: what puts a property here is that the cascade has to run again
        /// for the frame to be right, either because a descendant's computed value depends on it or
        /// because a stage between the cascade and the painter reads it.
        const CASCADED = 1 << 2;
        /// One of the four properties that decide where a box is drawn: `transform`, `translate`,
        /// `rotate`, `scale`, and the origin they are all resolved about.
        ///
        /// Apart from the cascade because what they move is geometry rather than paint, and the
        /// stage that turns a style into geometry is the fragment pass. Nothing between the cascade
        /// and that pass reads them, and no descendant inherits one — a descendant is carried along
        /// by the *matrix*, which the fragment pass composes down the tree from whatever it was
        /// given.
        const TRANSFORM = 1 << 3;
    }
}

impl AnimatedProperties {
    /// Whether everything moving here is expressible as a repaint of the box's own rectangle.
    ///
    /// An empty set is not: nothing is animating, so there is nothing to take any path.
    pub fn is_paint_only(self) -> bool {
        !self.is_empty() && !self.contains(Self::CASCADED) && !self.contains(Self::TRANSFORM)
    }

    /// Whether everything moving here is expressible by recomposing the box's fragments.
    ///
    /// True for a transform on its own and for a transform moving beside a colour, because the two
    /// overrides are written into different places and read by different stages: the colour is
    /// composed over the shared paint description at emission, and the transform is read while the
    /// fragment is placed. Neither needs the cascade, so an element doing both still needs neither.
    pub fn is_placement_only(self) -> bool {
        !self.contains(Self::CASCADED) && self.contains(Self::TRANSFORM)
    }
}

/// The summary one animated property belongs to.
fn kind_of(property: &OwnedPropertyDeclarationId) -> AnimatedProperties {
    let OwnedPropertyDeclarationId::Longhand(longhand) = property else {
        return AnimatedProperties::CASCADED;
    };
    match longhand {
        LonghandId::Opacity => AnimatedProperties::OPACITY,
        LonghandId::Transform
        | LonghandId::Translate
        | LonghandId::Rotate
        | LonghandId::Scale
        | LonghandId::TransformOrigin => AnimatedProperties::TRANSFORM,
        LonghandId::BackgroundColor
        | LonghandId::BorderTopColor
        | LonghandId::BorderRightColor
        | LonghandId::BorderBottomColor
        | LonghandId::BorderLeftColor
        | LonghandId::OutlineColor => AnimatedProperties::PAINT_COLOR,
        _ => AnimatedProperties::CASCADED,
    }
}

/// Which kinds of property `set`'s animations and transitions are moving at `now`.
pub(crate) fn properties(set: &ElementAnimationSet, now: f64) -> AnimatedProperties {
    let mut moved = AnimatedProperties::empty();
    for transition in &set.transitions {
        if !ticking(&transition.state) {
            continue;
        }
        moved |= kind_of(&transition.property_animation.property_id().to_owned());
    }
    if let Some(map) = set.get_value_map_for_active_animations(now) {
        for property in map.keys() {
            moved |= kind_of(property);
        }
    }
    moved
}

/// What one element's animations currently evaluate to, in the places they are written.
///
/// Two records rather than one because they are read by two different stages: the paint override is
/// composed over the shared paint description at the moment of emission, and the placement is read
/// while the element's fragments are composed. An element may produce both — a card that fades in
/// as it slides — and neither of them costs the cascade.
pub(crate) struct Sampled {
    /// The colours and alpha the animation is driving.
    pub(crate) paint: AnimOverride,
    /// Where the animation is putting the box, when it is moving it at all.
    pub(crate) placement: Option<AnimPlacement>,
    /// Whether what the animation is doing to the box cannot be expressed outside the cascade
    /// after all, whatever the property summary said.
    ///
    /// True for exactly one thing: an animation that starts or stops the element being transformed,
    /// rather than moving a transform it already has. Whether a box is transformed decides whether
    /// it establishes a stacking context and whether it is the containing block for the fixed and
    /// absolutely positioned boxes below it — two answers that are read from the *shared* style, by
    /// the box tree and by the painting order, and that an override written beside that style
    /// cannot change. So an animation crossing that line goes back through the cascade, where both
    /// answers are recomputed from a style that agrees with it.
    pub(crate) demoted: bool,
}

/// The values `set` currently overrides on an element whose cascade result is `base`.
///
/// The returned record holds only what an animation is actually driving, so an element whose
/// transition moves one colour produces one `Some` and three `None`s.
pub(crate) fn values(
    set: &ElementAnimationSet,
    context: &SharedStyleContext,
    base: &ServoArc<ComputedValues>,
) -> Sampled {
    let mut animated = base.clone();
    set.apply_active_animations(context, &mut animated);

    let placement = placement(base, &animated);
    let mut over = AnimOverride::new();
    if animated.get_effects().opacity != base.get_effects().opacity {
        over.opacity = Some(animated.get_effects().opacity.clamp(0.0, 1.0));
    }
    if animated.get_background().background_color != base.get_background().background_color {
        over.background_color = Some(resolved(
            &animated,
            &animated.get_background().background_color,
        ));
    }
    let border = animated.get_border();
    let was = base.get_border();
    if border.border_top_color != was.border_top_color
        || border.border_right_color != was.border_right_color
        || border.border_bottom_color != was.border_bottom_color
        || border.border_left_color != was.border_left_color
    {
        over.border_colors = Some([
            resolved(&animated, &border.border_top_color),
            resolved(&animated, &border.border_right_color),
            resolved(&animated, &border.border_bottom_color),
            resolved(&animated, &border.border_left_color),
        ]);
    }
    if animated.get_outline().outline_color != base.get_outline().outline_color {
        over.outline_color = Some(resolved(&animated, &animated.get_outline().outline_color));
    }
    let demoted = placement.is_err();
    Sampled {
        paint: over,
        placement: placement.unwrap_or_default(),
        demoted,
    }
}

/// Where the animation is putting the box, or `Err` when it may not be told outside the cascade.
///
/// The refusal is the important half. Every consumer of a transform other than the matrix itself
/// reads the *shared* style: the box tree asks whether this element is a containing block for the
/// positioned boxes below it, and the painting order asks whether it establishes a stacking context
/// of its own. Both questions are "is there a transform at all", not "which one", so an animation
/// that moves a transform it already has changes neither — and an animation that brings one into
/// existence, or takes the last one away, changes both. That one is refused here and cascades,
/// which is the only path that can move the shared style the two answers are read from.
fn placement(
    base: &ServoArc<ComputedValues>,
    animated: &ServoArc<ComputedValues>,
) -> Result<Option<AnimPlacement>, ()> {
    let was = base.get_box();
    let now = animated.get_box();
    if was.has_transform_or_perspective() != now.has_transform_or_perspective() {
        return Err(());
    }
    if now == was {
        return Ok(None);
    }
    Ok(Some(AnimPlacement::new(animated.clone_box())))
}

/// Whether an animation in this state is still to be advanced.
fn ticking(state: &style::animation::AnimationState) -> bool {
    use style::animation::AnimationState;
    matches!(state, AnimationState::Running | AnimationState::Pending)
}

/// A colour-valued property with `currentColor` resolved against the style it was read from.
///
/// It has to be resolved here rather than left for the painter, because the painter is given the
/// *shared* style: an override carrying an unresolved keyword would resolve against whatever colour
/// the elements sharing that style happen to have, which is the sharing bug one property over.
fn resolved(style: &ServoArc<ComputedValues>, color: &ColorValue) -> AbsoluteColor {
    color.resolve_to_absolute(&style.get_inherited_text().color)
}

#[cfg(test)]
mod tests {
    use servo_arc::Arc as ServoArc;
    use style::properties::{LonghandId, OwnedPropertyDeclarationId};
    use zgui_css::StyleDraft;
    use zgui_css::values::transform::TransformOperationValue;

    use super::{AnimatedProperties, kind_of, placement};

    /// A style whose `transform` is a translation of `by` device-independent pixels.
    fn translated(by: f32) -> ServoArc<style::properties::ComputedValues> {
        let mut style = StyleDraft::initial().build();
        let operations = vec![TransformOperationValue::TranslateX(
            style::values::computed::LengthPercentage::new_length(
                style::values::computed::Length::new(by),
            ),
        )];
        ServoArc::make_mut(&mut style).mutate_box().transform =
            style::values::generics::transform::GenericTransform(operations.into());
        style
    }

    #[test]
    fn a_transform_that_comes_into_existence_is_refused_the_placement_path() {
        // The one thing this tier may not do. Whether a box is transformed decides whether it
        // establishes a stacking context and whether it is the containing block for the positioned
        // boxes below it, and both are read from the shared style — which an override written
        // beside that style cannot move. So an animation crossing that line cascades instead.
        let none = StyleDraft::initial().build();
        assert!(placement(&none, &translated(10.0)).is_err());
        assert!(placement(&translated(10.0), &none).is_err());
    }

    #[test]
    fn a_transform_that_merely_moves_is_admitted() {
        let from = translated(10.0);
        let to = translated(40.0);
        let placed = placement(&from, &to).expect("a transform that was already there is admitted");
        let placed = placed.expect("a transform that moved produces a placement");
        assert_eq!(placed.group(), to.get_box());
    }

    #[test]
    fn a_transform_that_did_not_move_produces_no_placement() {
        // What the caller owes a fragment on is the value having *changed*. A placement handed
        // back for a transform holding still would recompose the box, damage its ink and rewrite
        // its hit entry on every frame the window runs.
        let held = translated(10.0);
        assert!(placement(&held, &held).expect("nothing crossed").is_none());
    }

    #[test]
    fn opacity_and_a_background_are_paint_only() {
        let set = kind_of(&OwnedPropertyDeclarationId::Longhand(LonghandId::Opacity))
            | kind_of(&OwnedPropertyDeclarationId::Longhand(
                LonghandId::BackgroundColor,
            ));
        assert!(set.is_paint_only());
    }

    #[test]
    fn a_transform_is_not() {
        // A transform moves the box and everything under it, so the repaint-only path cannot
        // express it: the fragment tree, the ink and the hit envelopes all have to be recomposed.
        assert!(
            !kind_of(&OwnedPropertyDeclarationId::Longhand(LonghandId::Transform)).is_paint_only()
        );
    }

    #[test]
    fn the_four_transform_properties_and_their_origin_are_one_kind() {
        // Resolved into one matrix, about one origin, so they take one path or none of them does.
        for longhand in [
            LonghandId::Transform,
            LonghandId::Translate,
            LonghandId::Rotate,
            LonghandId::Scale,
            LonghandId::TransformOrigin,
        ] {
            let kind = kind_of(&OwnedPropertyDeclarationId::Longhand(longhand));
            assert_eq!(kind, AnimatedProperties::TRANSFORM, "{longhand:?}");
            assert!(kind.is_placement_only());
        }
    }

    #[test]
    fn a_transform_beside_a_fade_still_needs_no_cascade() {
        // Two overrides in two places, read by two stages. Neither is the cascade.
        let both = kind_of(&OwnedPropertyDeclarationId::Longhand(LonghandId::Transform))
            | kind_of(&OwnedPropertyDeclarationId::Longhand(LonghandId::Opacity));
        assert!(both.is_placement_only());
        assert!(!both.is_paint_only());
    }

    #[test]
    fn a_transform_beside_a_width_takes_the_width_home() {
        // The rule is a union: one property the fragment pass cannot express takes the element
        // with it, exactly as it does for the paint-only path.
        let mixed = kind_of(&OwnedPropertyDeclarationId::Longhand(LonghandId::Transform))
            | kind_of(&OwnedPropertyDeclarationId::Longhand(LonghandId::Width));
        assert!(!mixed.is_placement_only());
        assert!(!mixed.is_paint_only());
    }

    #[test]
    fn an_inherited_colour_is_not() {
        // `color` is inherited, so an override written on one node would leave every descendant
        // drawing the value the node had before the animation started.
        assert!(!kind_of(&OwnedPropertyDeclarationId::Longhand(LonghandId::Color)).is_paint_only());
    }

    #[test]
    fn nothing_animating_takes_no_path_at_all() {
        assert!(!AnimatedProperties::empty().is_paint_only());
    }
}
