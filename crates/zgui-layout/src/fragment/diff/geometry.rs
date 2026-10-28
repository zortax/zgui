//! One fragment's composed geometry, and what a comparison against the previous one reports.

use zgui_geom::{Device, DevicePx, Edges, Rect, Size};

use crate::fragment::diff::Change;
use crate::fragment::{Fragment, FragmentExtra, FragmentFlags, build};

/// One fragment's geometry, before it is written.
pub(super) struct Geometry {
    /// The border box in local space.
    pub(super) border_box: Rect<DevicePx, Device>,
    /// The snapped border widths.
    pub(super) border: Edges<DevicePx>,
    /// The snapped padding widths.
    pub(super) padding: Edges<DevicePx>,
    /// The room the scrollbars take inside the padding.
    pub(super) scrollbar: Size<DevicePx, Device>,
    /// Everything it paints, in [`Geometry::anchor`]'s space.
    pub(super) ink: Rect<DevicePx, Device>,
    /// The same, in its own space.
    pub(super) local_ink: Rect<DevicePx, Device>,
    /// What painting, damage and hit testing branch on.
    pub(super) flags: FragmentFlags,
    /// The clip chain it is drawn under.
    pub(super) clip: zgui_scene::ClipId,
    /// The coordinate system it is drawn in.
    pub(super) transform: Option<zgui_scene::SpatialId>,
    /// The coordinate system its ink is measured in.
    pub(super) anchor: zgui_scene::SpatialId,
    /// The clip space and the two fingerprints, see [`FragmentExtra`].
    pub(super) extra: FragmentExtra,
    /// The stacking context it belongs to.
    pub(super) stacking: Option<zgui_scene::StackingContextId>,
    /// The scrollable region it moves with.
    pub(super) scroll: Option<zgui_scene::ScrollFrameId>,
    /// Whether it reads pixels outside every rectangle it writes.
    pub(super) reads_outside: bool,
}

impl Geometry {
    /// The padding box in local space, derived as [`Fragment::padding_box`] derives it.
    pub(super) fn padding_box(&self) -> Rect<DevicePx, Device> {
        build::padding_box_of(self.border_box, self.border)
    }

    /// The content box in local space, derived as [`Fragment::content_box`] derives it.
    pub(super) fn content_box(&self) -> Rect<DevicePx, Device> {
        build::content_box_of(self.padding_box(), self.padding, self.scrollbar)
    }
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
///
/// `held` is what the store holds beside `previous`, see [`FragmentExtra`].
pub(super) fn repositioned_within(
    previous: &Fragment,
    held: &FragmentExtra,
    next: &Geometry,
) -> bool {
    let plain = |flags: FragmentFlags| {
        flags.without(FragmentFlags::UNCOMPARED) == FragmentFlags::PAINTS_NOTHING
    };
    plain(next.flags)
        && plain(previous.flags)
        && previous.border_box.size == next.border_box.size
        && previous.border == next.border
        && previous.clip == next.clip
        && held.clip_transform == next.extra.clip_transform
        && previous.transform == next.transform
        && previous.anchor == next.anchor
        && held.transform_hash == next.extra.transform_hash
        && previous.stacking() == next.stacking
        && previous.scroll() == next.scroll
        && held.content_hash == next.extra.content_hash
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

/// Whether a fragment and the geometry replacing it differ in nothing an accessibility node
/// derives from a fragment except where it is and how large it is.
///
/// The projection reads a fragment's rectangle, its coordinate system and whether it clips its
/// children; everything else it states comes from the document. What a line holds and where it is
/// cut are painting, and a name is read off the text node rather than off the glyphs, so a piece
/// whose text was shaped again owes the tree its rectangle and nothing more.
pub(super) fn only_geometry_differs(previous: &Fragment, next: &Geometry) -> bool {
    previous.flags.contains(FragmentFlags::CLIPS_CHILDREN)
        == next.flags.contains(FragmentFlags::CLIPS_CHILDREN)
        && previous.transform == next.transform
}

/// What changed between a fragment and the geometry that replaces it.
///
/// `held` is what the store holds beside `previous`, see [`FragmentExtra`].
pub(super) fn compare(previous: &Fragment, held: &FragmentExtra, next: &Geometry) -> Change {
    // The border box, the border and the padding give the padding box and, but for the
    // scrollbars, the content box. Scrollbars wider than the room clamp to zero, so the derived
    // sizes are compared only when they differ.
    let same_shape = previous.border_box.size == next.border_box.size
        && previous.border == next.border
        && previous.padding == next.padding
        && (previous.scrollbar == next.scrollbar
            || previous.content_box().size == next.content_box().size)
        && previous.clip == next.clip
        && held.clip_transform == next.extra.clip_transform
        && previous.transform == next.transform
        // Ink in another space is another rectangle, however equal the numbers are.
        && previous.anchor == next.anchor
        // The name of a coordinate system is structural, so it is the same name for a box that has
        // been moved; without the fingerprint, a movement whose device ink happens to land where
        // the last one did — a square rotated through a right angle, a scale about its own centre —
        // would compare identical and never be redrawn.
        && held.transform_hash == next.extra.transform_hash
        && previous.stacking() == next.stacking
        && previous.scroll() == next.scroll
        // Where a line was cut short and what marks the cut. Both move without moving the line box:
        // narrowing the box a `white-space: nowrap` label sits in changes neither the line's width
        // nor its position, and changes exactly which of its clusters are painted.
        && held.content_hash == next.extra.content_hash
        && previous.flags.without(FragmentFlags::UNCOMPARED)
            == next.flags.without(FragmentFlags::UNCOMPARED);
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
    use crate::fragment::diff::Change;
    use crate::fragment::{Fragment, FragmentExtra};

    /// The geometry a fragment already holds, so that only what a test changes differs.
    fn same_as(fragment: &Fragment, extra: FragmentExtra) -> Geometry {
        Geometry {
            border_box: fragment.border_box,
            border: fragment.border,
            padding: fragment.padding,
            scrollbar: fragment.scrollbar,
            ink: fragment.ink,
            local_ink: fragment.local_ink,
            flags: fragment.flags,
            clip: fragment.clip,
            transform: fragment.transform,
            anchor: fragment.anchor,
            extra,
            stacking: fragment.stacking(),
            scroll: fragment.scroll(),
            reads_outside: false,
        }
    }

    /// A fragment that has been composed once, at the origin, drawing nothing in particular.
    fn composed() -> (Fragment, FragmentExtra) {
        let generation = zgui_arena::Generation::FIRST;
        let domain = zgui_arena::DomainId::FIRST;
        let fragment = Fragment::new(
            zgui_arena::Key::new(0, generation, domain),
            zgui_arena::Key::new(1, generation, domain),
            crate::fragment::FragmentKind::Box,
        );
        let extra = FragmentExtra {
            transform_hash: 7,
            ..FragmentExtra::NONE
        };
        (fragment, extra)
    }

    #[test]
    fn a_fragment_nothing_moved_is_identical() {
        let (fragment, extra) = composed();
        let next = same_as(&fragment, extra);
        assert_eq!(compare(&fragment, &extra, &next), Change::Identical);
    }

    #[test]
    fn a_matrix_that_changed_under_an_unchanged_name_is_a_change() {
        let (fragment, extra) = composed();
        let next = same_as(
            &fragment,
            FragmentExtra {
                transform_hash: 9,
                ..extra
            },
        );
        assert_eq!(
            compare(&fragment, &extra, &next),
            Change::Changed,
            "the name of a coordinate system does not move when the matrix under it does, so \
             nothing else here would have differed",
        );
    }

    #[test]
    fn a_scrollbar_that_narrows_the_content_box_is_a_change() {
        let (mut fragment, extra) = composed();
        fragment.border_box = zgui_geom::Rect::new(
            zgui_geom::Point::new(zgui_geom::DevicePx(0.0), zgui_geom::DevicePx(0.0)),
            zgui_geom::Size::new(zgui_geom::DevicePx(100.0), zgui_geom::DevicePx(50.0)),
        );
        let mut next = same_as(&fragment, extra);
        next.scrollbar = zgui_geom::Size::new(zgui_geom::DevicePx(12.0), zgui_geom::DevicePx(0.0));
        assert_eq!(next.content_box().size.width.0, 88.0);
        assert_eq!(compare(&fragment, &extra, &next), Change::Changed);
    }
}
