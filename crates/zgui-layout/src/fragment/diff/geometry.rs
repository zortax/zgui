//! One fragment's composed geometry, and what a comparison against the previous one reports.

use zgui_geom::{Device, DevicePx, Edges, Rect};

use crate::fragment::diff::Change;
use crate::fragment::{Fragment, FragmentFlags};

/// One fragment's geometry, before it is written.
pub(super) struct Geometry {
    /// The border box in local space.
    pub(super) border_box: Rect<DevicePx, Device>,
    /// The padding box in local space.
    pub(super) padding_box: Rect<DevicePx, Device>,
    /// The content box in local space.
    pub(super) content_box: Rect<DevicePx, Device>,
    /// The snapped border widths.
    pub(super) border: Edges<DevicePx>,
    /// The snapped padding widths.
    pub(super) padding: Edges<DevicePx>,
    /// Everything it paints, in device space.
    pub(super) ink: Rect<DevicePx, Device>,
    /// The same, in its own space.
    pub(super) local_ink: Rect<DevicePx, Device>,
    /// What painting, damage and hit testing branch on.
    pub(super) flags: FragmentFlags,
    /// The clip chain it is drawn under.
    pub(super) clip: zgui_scene::ClipId,
    /// The matrix that chain's rectangles were measured in.
    pub(super) clip_transform: Option<zgui_scene::SpatialId>,
    /// The coordinate system it is drawn in.
    pub(super) transform: Option<zgui_scene::SpatialId>,
    /// A fingerprint of the matrix that coordinate system resolves to.
    pub(super) transform_hash: u64,
    /// The stacking context it belongs to.
    pub(super) stacking: Option<zgui_scene::StackingContextId>,
    /// The scrollable region it moves with.
    pub(super) scroll: Option<zgui_scene::ScrollFrameId>,
    /// Whether it reads pixels outside every rectangle it writes.
    pub(super) reads_outside: bool,
    /// A fingerprint of what it draws that its rectangles do not describe.
    pub(super) content_hash: u64,
}

/// Whether a changed fragment is a paintless box that moved rigidly, or stood still, while its
/// inner boxes repositioned.
///
/// Such a fragment owes the frame no damage of its own: it painted nothing before and paints
/// nothing after, and what moved *inside* it is other fragments' geometry, which damages itself.
/// The virtualised list's pane is the case this names — a fragment as tall as every row there
/// will ever be, re-laid on every frame the window over it shifts, whose old-union-new ink used
/// to be the whole scrollport, every frame a row crossed an edge.
///
/// Held to fragments whose only flag is [`FragmentFlags::PAINTS_NOTHING`]: a clipping,
/// transformed, sticky, stacking or read-extent box couples to its surroundings in ways this
/// comparison does not measure, and each of those is rare enough to pay the two rectangles.
pub(super) fn repositioned_within(previous: &Fragment, next: &Geometry) -> bool {
    let plain = |flags: FragmentFlags| {
        flags.without(FragmentFlags::HAS_BLENDING_DESCENDANT) == FragmentFlags::PAINTS_NOTHING
    };
    plain(next.flags)
        && plain(previous.flags)
        && previous.border_box.size == next.border_box.size
        && previous.border == next.border
        && previous.clip == next.clip
        && previous.clip_transform == next.clip_transform
        && previous.transform == next.transform
        && previous.transform_hash == next.transform_hash
        && previous.stacking == next.stacking
        && previous.scroll == next.scroll
        && previous.content_hash == next.content_hash
        && moved_alike(previous.border_box, next.border_box, previous.ink, next.ink)
        && moved_alike(
            previous.border_box,
            next.border_box,
            previous.local_ink,
            next.local_ink,
        )
}

/// Whether `ink` moved exactly as far as the border box did, keeping its size.
fn moved_alike(
    border_was: Rect<DevicePx, Device>,
    border_is: Rect<DevicePx, Device>,
    ink_was: Rect<DevicePx, Device>,
    ink_is: Rect<DevicePx, Device>,
) -> bool {
    ink_was.size == ink_is.size
        && ink_is.origin.x.0 - ink_was.origin.x.0 == border_is.origin.x.0 - border_was.origin.x.0
        && ink_is.origin.y.0 - ink_was.origin.y.0 == border_is.origin.y.0 - border_was.origin.y.0
}

/// What changed between a fragment and the geometry that replaces it.
pub(super) fn compare(previous: &Fragment, next: &Geometry) -> Change {
    let same_shape = previous.border_box.size == next.border_box.size
        && previous.padding_box.size == next.padding_box.size
        && previous.content_box.size == next.content_box.size
        && previous.border == next.border
        && previous.padding == next.padding
        && previous.clip == next.clip
        && previous.clip_transform == next.clip_transform
        && previous.transform == next.transform
        // The name of a coordinate system is structural, so it is the same name for a box that has
        // been moved; without the fingerprint, a movement whose device ink happens to land where
        // the last one did — a square rotated through a right angle, a scale about its own centre —
        // would compare identical and never be redrawn.
        && previous.transform_hash == next.transform_hash
        && previous.stacking == next.stacking
        && previous.scroll == next.scroll
        // Where a line was cut short and what marks the cut. Both move without moving the line box:
        // narrowing the box a `white-space: nowrap` label sits in changes neither the line's width
        // nor its position, and changes exactly which of its clusters are painted.
        && previous.content_hash == next.content_hash
        && previous
            .flags
            .without(FragmentFlags::HAS_BLENDING_DESCENDANT)
            == next.flags.without(FragmentFlags::HAS_BLENDING_DESCENDANT);
    if !same_shape {
        return Change::Changed;
    }
    let moved = (
        next.border_box.origin.x.0 - previous.border_box.origin.x.0,
        next.border_box.origin.y.0 - previous.border_box.origin.y.0,
    );
    if moved == (0.0, 0.0) && previous.ink == next.ink {
        return Change::Identical;
    }
    // Everything it paints has to have moved by the same amount, or replaying its recorded
    // painting at an offset would put some of it in the wrong place.
    let ink_moved = (
        next.ink.origin.x.0 - previous.ink.origin.x.0,
        next.ink.origin.y.0 - previous.ink.origin.y.0,
    );
    if ink_moved == moved && previous.ink.size == next.ink.size {
        return Change::TranslatedOnly;
    }
    Change::Changed
}

#[cfg(test)]
mod tests {
    use super::{Geometry, compare};
    use crate::fragment::Fragment;
    use crate::fragment::diff::Change;

    /// The geometry a fragment already holds, so that only what a test changes differs.
    fn same_as(fragment: &Fragment) -> Geometry {
        Geometry {
            border_box: fragment.border_box,
            padding_box: fragment.padding_box,
            content_box: fragment.content_box,
            border: fragment.border,
            padding: fragment.padding,
            ink: fragment.ink,
            local_ink: fragment.local_ink,
            flags: fragment.flags,
            clip: fragment.clip,
            clip_transform: fragment.clip_transform,
            transform: fragment.transform,
            transform_hash: fragment.transform_hash,
            stacking: fragment.stacking,
            scroll: fragment.scroll,
            reads_outside: false,
            content_hash: fragment.content_hash,
        }
    }

    /// A fragment that has been composed once, at the origin, drawing nothing in particular.
    fn composed() -> Fragment {
        let generation = zgui_arena::Generation::FIRST;
        let domain = zgui_arena::DomainId::FIRST;
        let mut fragment = Fragment::new(
            zgui_arena::Key::new(0, generation, domain),
            zgui_arena::Key::new(1, generation, domain),
            crate::fragment::FragmentKind::Box,
        );
        fragment.transform_hash = 7;
        fragment
    }

    #[test]
    fn a_fragment_nothing_moved_is_identical() {
        let fragment = composed();
        let next = same_as(&fragment);
        assert_eq!(compare(&fragment, &next), Change::Identical);
    }

    #[test]
    fn a_matrix_that_changed_under_an_unchanged_name_is_a_change() {
        let fragment = composed();
        let next = Geometry {
            transform_hash: 9,
            ..same_as(&fragment)
        };
        assert_eq!(
            compare(&fragment, &next),
            Change::Changed,
            "the name of a coordinate system does not move when the matrix under it does, so \
             nothing else here would have differed",
        );
    }
}
