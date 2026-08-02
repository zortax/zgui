//! What the hit index knows about one fragment.

use zgui_dom::NodeKey;
use zgui_geom::{Corners, Device, DevicePx, Rect, Vec2};
use zgui_scene::{ClipId, DrawOrder, SpatialId};

use crate::fragment::FragKey;
use crate::fragment::hit::pointer_events::PointerEvents;

/// One fragment, as the hit index sees it.
///
/// Everything here is what answering "is this point on this fragment" needs, and nothing else: the
/// index holds no styles and reaches into no store while it answers, which is what lets a pointer
/// move cost a tree descent rather than a walk over the document.
///
/// # Which space it is expressed in
///
/// Every rectangle here is in the coordinate system [`HitEntry::space`] names, and not one of them
/// is in device pixels. That is the whole difference between an entry that has to be rewritten when
/// its box is animated and one that does not: a matrix is a property of the *space*, so an entry
/// that never mentions the device stays true for as long as the box occupies the same rectangle of
/// its own space, however that space is moving.
///
/// It is also the only representation that can be *checked*. An entry holding device coordinates is
/// kept current today only because fragment recomposition happens to rewrite it; the moment a phase
/// stops recomposing, the entry silently describes where the box used to be drawn, and no border
/// box, no transcript and no damage oracle can see it — the fragment did not move, the primitive is
/// drawn through the same node, and the pixels are right.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitEntry {
    /// The fragment this describes.
    pub frag: FragKey,
    /// The element it belongs to, if it has one. An anonymous box has none, and a hit on it
    /// resolves to whatever element contains it.
    pub node: Option<NodeKey>,
    /// Where this fragment sits in painting order.
    ///
    /// Carried rather than derived: the topmost fragment under a point is the last one painted, and
    /// the order that decides that is the same order the display list is emitted in. An index that
    /// invented its own would answer differently from what is on the screen.
    pub order: DrawOrder,
    /// The clip chain the fragment is drawn under. A point outside any link of it is not on the
    /// fragment however far inside its own rectangle it falls.
    pub clip: ClipId,
    /// The coordinate system the clip chain's rectangles were measured in.
    ///
    /// Held apart from [`HitEntry::space`] because the two are different spaces whenever the
    /// fragment carries a transform of its own: the clip belongs to whichever ancestor imposed it
    /// and was measured before this fragment moved. Testing the chain in the fragment's own space
    /// would let a translated box answer over the part of its ancestor's scrollport it was
    /// translated out of.
    pub clip_space: Option<SpatialId>,
    /// The coordinate system this entry's two rectangles are in.
    ///
    /// Also which of the index's trees the entry is filed in: entries are grouped by space so that
    /// a query maps the point into each space once instead of mapping every candidate rectangle out
    /// of one.
    ///
    /// `None` is the device itself, which is where a fragment that has never been told a coordinate
    /// system is.
    pub space: Option<SpatialId>,
    /// Whether the fragment takes pointer events at all.
    pub pointer_events: PointerEvents,
    /// The corner radii, so that a click on the corner of a rounded button misses it.
    pub radii: Corners<Vec2<DevicePx>>,
    /// The fragment's border box, in its own space.
    pub bounds: Rect<DevicePx, Device>,
    /// Everything the fragment paints, in the same space, which is what the index is keyed by.
    ///
    /// It contains `bounds` and is the same rectangle for the overwhelming majority of fragments;
    /// what makes it the key is that a shadow or an outline is drawn outside the border box and a
    /// hierarchy keyed by anything smaller would dismiss a subtree that does cover the point.
    pub envelope: Rect<DevicePx, Device>,
}

impl HitEntry {
    /// An entry for a fragment that is not clipped, not transformed and takes events.
    pub fn new(frag: FragKey, bounds: Rect<DevicePx, Device>) -> Self {
        Self {
            frag,
            node: None,
            order: 0,
            clip: ClipId::ROOT,
            clip_space: None,
            space: None,
            pointer_events: PointerEvents::Auto,
            radii: Corners::uniform(Vec2::splat(DevicePx(0.0))),
            bounds,
            envelope: bounds,
        }
    }

    /// Whether `point`, already mapped into this fragment's own space, lands on it.
    ///
    /// The rounded-corner test is here rather than in the caller because it is part of what the
    /// fragment *is*: a rounded button is not a rectangle, and a press on the square millimetre
    /// outside its curve has to fall through to whatever is behind it.
    pub fn covers(&self, point: zgui_geom::Point<DevicePx, Device>) -> bool {
        covers_rounded(self.bounds, self.radii, point)
    }
}

/// Whether a point lands inside a rounded rectangle.
///
/// Shared with the clip chain, whose links are rounded rectangles too: a click inside a rounded
/// scrollport's corner must miss for the same reason it misses on a rounded button, and two
/// implementations of that would eventually disagree by a pixel.
pub(crate) fn covers_rounded(
    rect: Rect<DevicePx, Device>,
    radii: Corners<Vec2<DevicePx>>,
    point: zgui_geom::Point<DevicePx, Device>,
) -> bool {
    rect.contains(point) && !outside_a_corner(rect, radii, point)
}

/// Whether a point inside a rectangle falls outside one of its rounded corners.
fn outside_a_corner(
    rect: Rect<DevicePx, Device>,
    radii: Corners<Vec2<DevicePx>>,
    point: zgui_geom::Point<DevicePx, Device>,
) -> bool {
    let corners = [
        (
            radii.top_left,
            rect.left().0 + radii.top_left.x.0,
            rect.top().0 + radii.top_left.y.0,
            point.x.0 < rect.left().0 + radii.top_left.x.0,
            point.y.0 < rect.top().0 + radii.top_left.y.0,
        ),
        (
            radii.top_right,
            rect.right().0 - radii.top_right.x.0,
            rect.top().0 + radii.top_right.y.0,
            point.x.0 > rect.right().0 - radii.top_right.x.0,
            point.y.0 < rect.top().0 + radii.top_right.y.0,
        ),
        (
            radii.bottom_right,
            rect.right().0 - radii.bottom_right.x.0,
            rect.bottom().0 - radii.bottom_right.y.0,
            point.x.0 > rect.right().0 - radii.bottom_right.x.0,
            point.y.0 > rect.bottom().0 - radii.bottom_right.y.0,
        ),
        (
            radii.bottom_left,
            rect.left().0 + radii.bottom_left.x.0,
            rect.bottom().0 - radii.bottom_left.y.0,
            point.x.0 < rect.left().0 + radii.bottom_left.x.0,
            point.y.0 > rect.bottom().0 - radii.bottom_left.y.0,
        ),
    ];
    for (radius, centre_x, centre_y, before_x, before_y) in corners {
        if radius.x.0 <= 0.0 || radius.y.0 <= 0.0 || !before_x || !before_y {
            continue;
        }
        let dx = (point.x.0 - centre_x) / radius.x.0;
        let dy = (point.y.0 - centre_y) / radius.y.0;
        if dx * dx + dy * dy > 1.0 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use zgui_arena::{ArenaKind, DocumentId, DomainId, Generation, Key};
    use zgui_geom::{Corners, DevicePx, Point, Rect, Size, Vec2};

    use super::HitEntry;

    /// A fragment name, for an entry that has to have one.
    fn key() -> crate::fragment::FragKey {
        Key::new(
            1,
            Generation::FIRST,
            DomainId::new(DocumentId::FIRST, ArenaKind::new(2).expect("a valid arena")),
        )
    }

    #[test]
    fn a_square_entry_covers_every_point_inside_it() {
        let entry = HitEntry::new(
            key(),
            Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(10.0), DevicePx(10.0)),
            ),
        );
        assert!(entry.covers(Point::new(DevicePx(0.5), DevicePx(0.5))));
        assert!(!entry.covers(Point::new(DevicePx(11.0), DevicePx(5.0))));
    }

    #[test]
    fn a_rounded_corner_is_missed_where_the_square_one_would_be_hit() {
        let mut entry = HitEntry::new(
            key(),
            Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(20.0), DevicePx(20.0)),
            ),
        );
        entry.radii = Corners::uniform(Vec2::splat(DevicePx(8.0)));
        // The very corner is outside the curve; a point on the straight edge is not.
        assert!(!entry.covers(Point::new(DevicePx(0.2), DevicePx(0.2))));
        assert!(entry.covers(Point::new(DevicePx(10.0), DevicePx(0.2))));
    }
}
