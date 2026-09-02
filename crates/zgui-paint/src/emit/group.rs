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

use zgui_layout::{Fragment, FragmentFlags};
use zgui_profile::{Counter, counter};
use zgui_scene::{BackdropFilter, ClipId, GroupBoundary, Scene, SpatialId};

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
    let blending_below = fragment
        .flags
        .contains(FragmentFlags::HAS_BLENDING_DESCENDANT);
    if fragment.subtree_disjoint && !blending_below {
        return Isolation::Folded(style.group.opacity);
    }
    Isolation::Target
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
    let bounds = fragment.subtree_ink;
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
    boundary.transform = fragment.transform.filter(|id| *id != SpatialId::VIEWPORT);
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
pub fn backdrop(
    scene: &mut Scene,
    style: &PaintStyle,
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
    let filter = BackdropFilter::new(fragment.border_box, filters).clipped(clip);
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
}
