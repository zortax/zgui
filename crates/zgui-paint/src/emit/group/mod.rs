//! Groups: the subtrees that have to be drawn into a target of their own and composited once.
//!
//! A group is drawn through the effect of the order node the compositor sync gives its box.
//!
//! # The decision, and the one part of it that is geometric
//!
//! A blend mode, a filter, an explicit `isolation`, a `clip-path` shape and a three-dimensional
//! transform all need a boundary whatever the content is. Opacity is the one that sometimes does
//! not: double-darkening only happens when two primitives in the subtree overlap, and when they do
//! not, folding the alpha into each primitive's own paint produces the same pixels for the price of
//! nothing.
//!
//! That fold is decided over the fragment tree's own ink, on the layout stage's unwind, and read
//! here. It is deliberately *not* decided over the primitives a frame emitted: a frame that painted
//! half a subtree would answer differently from one that painted all of it, and the two would differ
//! by a pixel — which is exactly what a damage-correctness comparison exists to catch.
//!
//! # And the clause that makes the fold conditional
//!
//! A blending descendant needs its boundary however the geometry falls, because it blends against
//! its nearest ancestor stacking context and not against the page. Without that clause, wrapping a
//! blend element in `position: relative; z-index: 0` — the universally taught fix — would isolate in
//! other engines and not in this one, and whether it did would depend on where a sibling happened to
//! sit.

mod filter;

use std::sync::Arc;

use zgui_layout::{Fragment, FragmentFlags};
use zgui_scene::{BackdropFilter, ClipId, ClipLink, Scene, SpatialId};

use crate::lower::PaintStyle;

/// The highest opacity that draws nothing a display shows: it moves no 8-bit channel by half a
/// step, so every pixel under it rounds to the value it had. A group at or below it folds to zero.
pub const INVISIBLE_OPACITY: f32 = 0.5 / 255.0;

/// What a fragment's group properties come to once its subtree has been taken into account.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Isolation {
    /// No target: the subtree is drawn straight onto whatever it is over.
    None,
    /// No target, and the group's alpha multiplied into each primitive's own paint.
    ///
    /// Carries the alpha so that the emitter has one number to apply rather than having to ask
    /// again what the fold decided.
    Folded(f32),
    /// A target of its own, composited once when the group closes.
    Target,
}

impl Isolation {
    /// The alpha to multiply into each primitive's paint under this decision.
    pub fn alpha(self) -> f32 {
        match self {
            Self::Folded(alpha) => alpha,
            Self::None | Self::Target => 1.0,
        }
    }

    /// Whether a target is allocated.
    pub fn needs_target(self) -> bool {
        self == Self::Target
    }
}

/// Decides how one fragment's subtree is composited.
///
/// The three inputs are the style's own demands, whether the subtree's ink is pairwise disjoint, and
/// whether anything below it blends — the last two both read off the fragment, where the layout
/// stage folded them.
pub fn isolation(style: &PaintStyle, fragment: &Fragment) -> Isolation {
    if style.needs_group() {
        return Isolation::Target;
    }
    // A filter isolates what blends below it, whatever the filter does to the pixels.
    let blending_below = fragment
        .flags
        .contains(FragmentFlags::HAS_BLENDING_DESCENDANT);
    if style.group.isolates_blending && blending_below {
        return Isolation::Target;
    }
    // A backdrop filter makes the box a backdrop root: what reads the backdrop below it reads the
    // box's own content and no further. Without such a reader the group is the identity, and the
    // box draws straight onto what is beneath it.
    if fragment.flags.contains(FragmentFlags::READS_BACKDROP_BELOW)
        && (style.group.isolates_blending
            || style
                .shader
                .as_ref()
                .is_some_and(|shader| shader.backdrop.is_some()))
    {
        return Isolation::Target;
    }
    if style.group.opacity >= 1.0 {
        return Isolation::None;
    }
    // Nothing times anything is nothing. A group at zero contributes no pixel however its subtree
    // is arranged, so neither overlapping children nor a blending one can make a target tell a
    // different story from the fold — and the fold is what lets the emitter recognise a subtree
    // that has vanished and stop pushing primitives for it. A group too faint to move a channel by
    // half a step shows the same nothing.
    if style.group.opacity <= INVISIBLE_OPACITY {
        return Isolation::Folded(0.0);
    }
    if fragment.subtree_disjoint && !blending_below {
        return Isolation::Folded(style.group.opacity);
    }
    Isolation::Target
}

/// What a group composites its subtree with, in the compositor's vocabulary.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupEffect {
    /// The clip chain the composited group is cut to.
    pub clip: ClipId,
    /// The multiplier on the group.
    pub opacity: f32,
    /// How the group is composited onto what is beneath it.
    pub blend: compositor_types::BlendMode,
    /// The filters applied to the group's content, in sequence.
    pub filters: Arc<[compositor_types::Filter]>,
    /// Whether the subtree is drawn into a group whatever the other fields say.
    pub isolate: bool,
    /// How many filter steps the compositor has no counterpart for, and which are left out.
    pub omitted: usize,
}

/// Returns the effect a fragment's subtree is drawn with under `isolation`, or `None` when it is
/// drawn in place.
///
/// A subtree folded to zero alpha is drawn with an opacity of zero, which draws nothing.
pub fn effect(
    style: &PaintStyle,
    fragment: &Fragment,
    isolation: Isolation,
) -> Option<GroupEffect> {
    match isolation {
        Isolation::Target => {
            let mut filters = Vec::new();
            let mut omitted = filter::convert(&style.group.filters, &mut filters);
            omitted += usize::from(
                style
                    .shader
                    .as_ref()
                    .is_some_and(|shader| shader.filter.is_some()),
            );
            Some(GroupEffect {
                clip: fragment.clip,
                opacity: style.group.opacity,
                blend: filter::blend(style.group.blend),
                filters: filters.into(),
                isolate: true,
                omitted,
            })
        }
        Isolation::Folded(alpha) if alpha <= 0.0 => Some(GroupEffect {
            clip: ClipId::ROOT,
            opacity: 0.0,
            blend: compositor_types::BlendMode::NORMAL,
            filters: Arc::from([]),
            isolate: false,
            omitted: 0,
        }),
        Isolation::None | Isolation::Folded(_) => None,
    }
}

/// Emits a fragment's `backdrop-filter`, which samples what has already been drawn beneath it.
///
/// It is pushed *before* the group's own content, because what it reads is what is under the group
/// and not what the group draws.
///
/// The filtered result is cut to the rounded border box, as the box's background is. `computed` is
/// the box's computed style, which gives the radii.
pub fn backdrop(
    scene: &mut Scene,
    style: &PaintStyle,
    computed: &zgui_css::ComputedStyle,
    fragment: &Fragment,
    clip: ClipId,
    shaders: &dyn crate::content::shader::ShaderSource,
    scale: f32,
) -> usize {
    let mut filters = style.group.backdrop.clone();
    // After the chain the `backdrop-filter` property wrote, for the reason a filter effect goes
    // after the `filter` chain: the two are separate properties with no order between them.
    if let Some(step) = crate::emit::shader::filter_step(
        scene,
        style
            .shader
            .as_ref()
            .and_then(|shader| shader.backdrop.as_ref()),
        shaders,
        scale,
    ) {
        filters.push(step);
    }
    if filters.is_empty() {
        return 0;
    }
    // The border box is in the box's own coordinate system, as every other rectangle it paints.
    let space = fragment.transform.filter(|id| *id != SpatialId::VIEWPORT);
    let radii = crate::lower::border::radii_of(computed, fragment.border_box, scale);
    let corners = ClipLink::shaped(
        fragment.border_box,
        radii,
        style.corner_shape,
        space.unwrap_or(SpatialId::VIEWPORT),
    );
    // The clip is unnamed: the box owner's name is the clip it gives its children. A replay
    // re-interns a noted clip at the position the content moved to.
    let clip = if corners.is_rounded() {
        let rounded = scene.clips.push(clip, corners);
        scene.note_minted_clip(rounded);
        rounded
    } else {
        clip
    };
    let filter = BackdropFilter::new(fragment.border_box, filters)
        .clipped(clip)
        .in_space(space);
    usize::from(scene.push_backdrop(filter).is_some())
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;
    use zgui_layout::{Fragment, FragmentFlags, FragmentKind};

    use super::{Isolation, isolation};
    use crate::lower::{PaintStyle, lower};

    /// A minted key, for a test that needs a name and not a stored value.
    fn key<T>(index: u32) -> zgui_arena::Key<T> {
        zgui_arena::Key::new(
            index,
            zgui_arena::Generation::new(1).expect("one is a generation"),
            zgui_arena::DomainId::FIRST,
        )
    }

    /// A fragment with the given disjointness and blending answers.
    fn fragment(disjoint: bool, blending: bool) -> Fragment {
        let mut fragment = Fragment::new(key(0), key(0), FragmentKind::Box);
        fragment.subtree_disjoint = disjoint;
        if blending {
            fragment.flags = fragment.flags.union(FragmentFlags::HAS_BLENDING_DESCENDANT);
        }
        fragment
    }

    /// The initial style with a given opacity.
    fn translucent(opacity: f32) -> PaintStyle {
        let mut style = lower(&StyleDraft::initial().build(), 1.0);
        style.group.opacity = opacity;
        style
    }

    #[test]
    fn an_opaque_box_needs_nothing() {
        assert_eq!(
            isolation(&translucent(1.0), &fragment(true, false)),
            Isolation::None
        );
    }

    #[test]
    fn a_translucent_disjoint_subtree_folds_its_alpha() {
        assert_eq!(
            isolation(&translucent(0.5), &fragment(true, false)),
            Isolation::Folded(0.5)
        );
    }

    #[test]
    fn a_translucent_overlapping_subtree_takes_a_target() {
        assert_eq!(
            isolation(&translucent(0.5), &fragment(false, false)),
            Isolation::Target,
            "two overlapping children under a half-transparent parent would darken twice"
        );
    }

    #[test]
    fn a_blending_descendant_takes_a_target_however_the_geometry_falls() {
        // This is the `position: relative; z-index: 0` wrapper. Without the clause, whether a
        // document isolates would depend on where an unrelated sibling happened to sit.
        assert_eq!(
            isolation(&translucent(0.5), &fragment(true, true)),
            Isolation::Target
        );
    }

    /// A fade that rests just above zero: it moves no channel, so it draws nothing either.
    #[test]
    fn a_group_too_faint_to_move_a_channel_folds_to_nothing() {
        assert_eq!(
            isolation(&translucent(0.001), &fragment(false, false)),
            Isolation::Folded(0.0)
        );
        assert_eq!(
            isolation(&translucent(0.01), &fragment(false, false)),
            Isolation::Target,
            "an opacity of 0.01 moves a white channel over black by two and a half steps"
        );
    }

    /// A hidden mark inside a control: its siblings overlap it, and it still draws nothing.
    #[test]
    fn a_fully_transparent_group_folds_whatever_its_subtree_looks_like() {
        assert_eq!(
            isolation(&translucent(0.0), &fragment(false, true)),
            Isolation::Folded(0.0),
            "a target composited at zero and a fold of zero put the same nothing on the screen, \
             and only the fold lets the emitter skip the primitives"
        );
    }

    #[test]
    fn a_filter_takes_a_target_at_full_opacity() {
        let mut style = translucent(1.0);
        style.group.filters.push(zgui_scene::Filter::Blur(2.0));
        assert_eq!(isolation(&style, &fragment(true, false)), Isolation::Target);
    }

    /// The style of a box that declared a filter whose chain lowered to nothing.
    fn identity_filter() -> PaintStyle {
        let mut style = translucent(1.0);
        style.group.isolates_blending = true;
        style
    }

    #[test]
    fn an_identity_filter_isolates_a_blending_subtree() {
        assert_eq!(
            isolation(&identity_filter(), &fragment(true, true)),
            Isolation::Target
        );
    }

    #[test]
    fn an_identity_filter_over_nothing_that_blends_needs_no_target() {
        assert_eq!(
            isolation(&identity_filter(), &fragment(true, false)),
            Isolation::None
        );
    }

    /// The style of a box with a `backdrop-filter`.
    fn frosted() -> PaintStyle {
        let mut style = translucent(1.0);
        style.group.backdrop.push(zgui_scene::Filter::Blur(4.0));
        style.group.isolates_blending = true;
        style
    }

    #[test]
    fn a_backdrop_with_nothing_below_reading_the_backdrop_needs_no_target() {
        // The backdrop is drawn before the group opens, so a group around the box's own content
        // composites that content unchanged.
        assert_eq!(
            isolation(&frosted(), &fragment(false, false)),
            Isolation::None
        );
    }

    #[test]
    fn a_backdrop_with_a_backdrop_reader_below_takes_a_target() {
        // A backdrop root: what reads the backdrop below it reads the box's content and no further.
        let mut below = fragment(false, false);
        below.flags = below.flags.union(FragmentFlags::READS_BACKDROP_BELOW);
        assert_eq!(isolation(&frosted(), &below), Isolation::Target);
        assert_eq!(
            isolation(&frosted(), &fragment(false, true)),
            Isolation::Target,
            "and so does a blending descendant"
        );
    }

    /// A rectangle in device pixels.
    fn rect(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device> {
        zgui_geom::Rect::new(
            zgui_geom::Point::new(zgui_geom::DevicePx(x), zgui_geom::DevicePx(y)),
            zgui_geom::Size::new(zgui_geom::DevicePx(width), zgui_geom::DevicePx(height)),
        )
    }

    /// A backdrop is measured in the box's own coordinate system, as the box's other rectangles
    /// are, so the renderer can put it where the system is in the frame it draws.
    #[test]
    fn a_backdrop_is_measured_in_the_coordinate_system_of_its_box() {
        let mut style = translucent(1.0);
        style.group.backdrop.push(zgui_scene::Filter::Blur(4.0));
        let mut fragment = fragment(true, false);
        fragment.border_box = rect(10.0, 10.0, 40.0, 30.0);
        let mut scene = zgui_scene::Scene::new();
        scene.begin_frame(zgui_geom::Size::new(200, 200));
        let owner = zgui_scene::PropertyOwner::new(7).expect("a handle");
        let own = zgui_scene::OwnSpace::of(
            Some(zgui_geom::Matrix4::translation(100.0, 0.0, 0.0)),
            None,
            false,
        );
        let viewport = scene.spatial.viewport();
        let space = scene.spatial.space_of(viewport, owner, own);
        fragment.transform = Some(space);
        let pushed = super::backdrop(
            &mut scene,
            &style,
            &StyleDraft::initial().build(),
            &fragment,
            zgui_scene::ClipId::ROOT,
            &crate::content::shader::NoShaders,
            1.0,
        );
        assert_eq!(pushed, 1);
        let backdrop = &scene.primitives.backdrops[0];
        assert_eq!(backdrop.transform, Some(space));
        assert_eq!(backdrop.bounds, rect(10.0, 10.0, 40.0, 30.0));
    }
}
