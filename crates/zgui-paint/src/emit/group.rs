//! Groups: the subtrees that have to be drawn into a target of their own and composited once.
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

use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_layout::{Fragment, FragmentFlags};
use zgui_profile::{Counter, counter};
use zgui_scene::{BackdropFilter, ClipId, ClipLink, GroupBoundary, Scene, SpatialId};

use crate::lower::PaintStyle;

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
    // that has vanished and stop pushing primitives for it.
    if style.group.opacity <= 0.0 {
        return Isolation::Folded(0.0);
    }
    if fragment.subtree_disjoint && !blending_below {
        return Isolation::Folded(style.group.opacity);
    }
    Isolation::Target
}

/// What a group writes, in the coordinate system `space` it composites under.
///
/// The fragment's subtree ink is in its anchor's space, through the matrix `space` had when the
/// fragment was composed, and the anchor's matrix puts it on the device. A place write moves
/// `space` and composes nothing, so a device rectangle goes stale while the content drawn under
/// `space` moves on. In the coordinates of `space` the rectangle moves with the content, as the
/// rectangle of every primitive does.
///
/// The fragment's own ink and local ink are one rectangle on the two sides of the composition
/// matrix, which gives the way back for a translation and a scale. A matrix that turns, skews or
/// flips, or a fragment with no ink of its own, maps back through the current matrix instead, as
/// the box around the result.
fn local_bounds(scene: &Scene, fragment: &Fragment, space: SpatialId) -> Rect<DevicePx, Device> {
    let device = fragment.on_device_in(fragment.subtree_ink, &scene.spatial);
    let Some(matrix) = scene.spatial.resolve(space) else {
        return device;
    };
    let [x_axis, y_axis, ..] = matrix.columns;
    let axis_aligned = x_axis[1] == 0.0 && y_axis[0] == 0.0 && x_axis[0] > 0.0 && y_axis[1] > 0.0;
    let (ink, local) = (
        fragment.on_device_in(fragment.ink, &scene.spatial),
        fragment.local_ink,
    );
    let fits = |device: f32, local: f32| device > 0.0 && local > 0.0;
    if !axis_aligned
        || !fits(ink.size.width.0, local.size.width.0)
        || !fits(ink.size.height.0, local.size.height.0)
    {
        return matrix.invert().map_or(device, |inverse| {
            zgui_geom::transformed_bounds(&inverse, device)
        });
    }
    let scale_x = local.size.width.0 / ink.size.width.0;
    let scale_y = local.size.height.0 / ink.size.height.0;
    Rect::new(
        Point::new(
            DevicePx(local.origin.x.0 + (device.origin.x.0 - ink.origin.x.0) * scale_x),
            DevicePx(local.origin.y.0 + (device.origin.y.0 - ink.origin.y.0) * scale_y),
        ),
        Size::new(
            DevicePx(device.size.width.0 * scale_x),
            DevicePx(device.size.height.0 * scale_y),
        ),
    )
}

/// Opens a group for a fragment, and returns the marker to close it with.
///
/// The marker is returned rather than looked up again on the way out, because a group's closing
/// marker must be the same shape as its opening one: half a pair leaves a target open, or composites
/// one that was never begun.
pub fn open(
    scene: &mut Scene,
    style: &PaintStyle,
    fragment: &Fragment,
    clip: ClipId,
    shaders: &dyn crate::content::shader::ShaderSource,
    scale: f32,
) -> GroupBoundary {
    let transform = fragment.transform.filter(|id| *id != SpatialId::VIEWPORT);
    let bounds = transform.map_or(fragment.subtree_ink, |space| {
        local_bounds(scene, fragment, space)
    });
    let mut filters = style.group.filters.clone();
    // After the chain the `filter` property wrote, rather than among it: the two are separate
    // properties with no order between them, and the step nothing can look inside goes last.
    if let Some(step) = crate::emit::shader::filter_step(
        scene,
        style
            .shader
            .as_ref()
            .and_then(|shader| shader.filter.as_ref()),
        shaders,
        scale,
    ) {
        filters.push(step);
    }
    let mut boundary =
        GroupBoundary::start(bounds, style.group.opacity, style.group.blend, filters).clipped(clip);
    boundary.transform = transform;
    counter::bump(Counter::GroupTargets);
    scene.push_group(boundary.clone());
    boundary
}

/// Closes a group opened by [`open`].
pub fn close(scene: &mut Scene, boundary: &GroupBoundary) {
    scene.push_group(boundary.end());
}

/// Emits a fragment's `backdrop-filter`, which samples what has already been drawn beneath it.
///
/// It is pushed *before* the group's own content, because what it reads is what is under the group
/// and not what the group draws.
///
/// The filtered result is cut to the rounded border box, as the box's background is. `computed` is
/// the box's computed style, which gives the radii.
#[expect(
    clippy::too_many_arguments,
    reason = "a backdrop is described by the style, the box and the clip it is cut to, and a \
              struct for the set would be built at one call site and read at one other"
)]
pub fn backdrop(
    scene: &mut Scene,
    style: &PaintStyle,
    computed: &zgui_css::ComputedStyle,
    fragment: &Fragment,
    clip: ClipId,
    shaders: &dyn crate::content::shader::ShaderSource,
    scale: f32,
    keeping: bool,
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
    let mut filter = BackdropFilter::new(fragment.border_box, filters)
        .clipped(clip)
        .in_space(space);
    // A backdrop the expansion chose may read the copy kept from the last frame; the rest renew
    // it. See [`BackdropFilter::keeping_its_capture`].
    if keeping {
        filter = filter.keeping_its_capture();
    }
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

    /// A slide: the fragment was composed with its system 40 pixels right, and a place write has
    /// since moved the system back to where layout put it. The group's bounds are in the system's
    /// own coordinates, so they do not depend on where the system is now.
    #[test]
    fn a_group_is_bounded_in_its_own_coordinates_whatever_its_system_moved_to() {
        let mut fragment = fragment(false, false);
        fragment.local_ink = rect(10.0, 10.0, 20.0, 20.0);
        fragment.ink = rect(50.0, 10.0, 20.0, 20.0);
        fragment.subtree_ink = rect(45.0, 5.0, 40.0, 30.0);
        for now in [40.0, 0.0] {
            let mut scene = zgui_scene::Scene::new();
            let owner = zgui_scene::PropertyOwner::new(7).expect("a handle");
            let own = zgui_scene::OwnSpace::of(
                Some(zgui_geom::Matrix4::translation(now, 0.0, 0.0)),
                None,
                false,
            );
            let viewport = scene.spatial.viewport();
            let space = scene.spatial.space_of(viewport, owner, own);
            assert_eq!(
                super::local_bounds(&scene, &fragment, space),
                rect(5.0, 5.0, 40.0, 30.0),
                "with the system at {now}"
            );
        }
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
            false,
        );
        assert_eq!(pushed, 1);
        let backdrop = &scene.primitives.backdrops[0];
        assert_eq!(backdrop.transform, Some(space));
        assert_eq!(backdrop.bounds, rect(10.0, 10.0, 40.0, 30.0));
    }
}
