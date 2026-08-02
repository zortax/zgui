//! The clip trie's structural promises.

use zgui_atlas::{AtlasKey, AtlasTile, TextureId, TextureKind, TileId};
use zgui_geom::{Device, DevicePx, Point, Rect, Size, Vec2};

use crate::clip::{ClipLink, ClipTable, MaskSource};
use crate::id::ClipId;
use crate::spatial::SpatialId;

/// A device rectangle.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// A coverage tile, for the mask links.
fn tile() -> AtlasTile {
    let _ = AtlasKey::new(0, TextureKind::Mono);
    AtlasTile {
        texture: TextureId::new(TextureKind::Mono, 0),
        tile: TileId(1),
        bounds: Rect::new(Point::new(0, 0), Size::new(8, 8)),
    }
}

#[test]
fn the_root_chain_is_pinned_at_a_known_id() {
    let mut clips = ClipTable::rooted();
    clips.begin_frame();
    clips.begin_frame();
    assert_eq!(clips.evict_least_recently_used(), 0);
    assert!(clips.contains(ClipId::ROOT));
    assert_eq!(clips.depth(ClipId::ROOT), 0);
}

#[test]
fn chains_sharing_a_prefix_share_their_nodes() {
    let mut clips = ClipTable::rooted();
    let card = clips.only(ClipLink::rect(rect(0.0, 0.0, 100.0, 100.0)));
    let left = clips.push(card, ClipLink::rect(rect(0.0, 0.0, 10.0, 10.0)));
    let right = clips.push(card, ClipLink::rect(rect(20.0, 0.0, 10.0, 10.0)));

    assert_ne!(left, right);
    assert_eq!(clips.common_ancestor(left, right), card);
    assert_eq!(clips.depth(left), 2);
    // root, card, left, right — and no duplicate of `card`.
    assert_eq!(clips.len(), 4);
}

#[test]
fn a_residual_is_the_part_below_the_shared_chain() {
    let mut clips = ClipTable::rooted();
    let outer = clips.only(ClipLink::rect(rect(0.0, 0.0, 100.0, 100.0)));
    let inner_link = ClipLink::rounded(rect(4.0, 4.0, 8.0, 8.0), Vec2::splat(DevicePx(4.0)));
    let inner = clips.push(outer, inner_link);

    assert_eq!(clips.residual(inner, inner), ClipId::ROOT);
    let residual = clips.residual(inner, outer);
    assert_eq!(clips.depth(residual), 1);
    assert_eq!(clips.links(residual), vec![inner_link]);
}

#[test]
fn resolution_intersects_every_rectangle_and_keeps_two_rounded_tests() {
    let mut clips = ClipTable::rooted();
    let radius = Vec2::splat(DevicePx(6.0));
    let first = clips.only(ClipLink::rounded(rect(0.0, 0.0, 100.0, 50.0), radius));
    let second = clips.push(
        first,
        ClipLink::rounded(rect(20.0, 0.0, 100.0, 50.0), radius),
    );

    let resolved = clips.resolve(second);
    assert_eq!(resolved.aabb, [20.0, 0.0, 80.0, 50.0]);
    assert_eq!(resolved.rounded_count, 2);
    assert!(!clips.needs_group_target(second));

    let third = clips.push(
        second,
        ClipLink::rounded(rect(0.0, 0.0, 40.0, 40.0), radius),
    );
    assert!(
        clips.needs_group_target(third),
        "a third rounded link must be promoted, never dropped"
    );
    assert_eq!(
        clips.resolve(third).aabb,
        [20.0, 0.0, 20.0, 40.0],
        "the rectangle still narrows even when the rounded test cannot be applied inline"
    );
}

#[test]
fn an_empty_intersection_is_visible_as_such() {
    let mut clips = ClipTable::rooted();
    let left = clips.only(ClipLink::rect(rect(0.0, 0.0, 10.0, 10.0)));
    let disjoint = clips.push(left, ClipLink::rect(rect(50.0, 0.0, 10.0, 10.0)));
    assert!(clips.resolve(disjoint).is_empty());
    assert!(!clips.resolve(left).is_empty());
}

#[test]
fn a_raster_mask_is_the_one_link_a_vector_scene_cannot_express() {
    let mut clips = ClipTable::rooted();
    let path_mask = clips.only(ClipLink::Mask {
        tile: tile(),
        transform: SpatialId::VIEWPORT,
        source: MaskSource::Path,
    });
    let raster_mask = clips.only(ClipLink::Mask {
        tile: tile(),
        transform: SpatialId::VIEWPORT,
        source: MaskSource::Raster,
    });

    assert!(clips.is_expressible_in_vector_scene(path_mask));
    assert!(!clips.is_expressible_in_vector_scene(raster_mask));
    assert!(clips.is_expressible_in_vector_scene(ClipId::ROOT));
}

#[test]
fn the_unbounded_clip_admits_a_surface_sized_rectangle() {
    let clips = ClipTable::rooted();
    let root = clips.resolve(ClipId::ROOT);
    assert!(!root.is_empty());
    assert!(root.left() < 0.0 && root.right() > 8192.0);
    assert_eq!(root.rounded_count, 0);
    assert_eq!(root.mask, None);
}

#[test]
fn a_thousand_scrolls_grow_no_clip_entry() {
    // A scrollport that stays where it is, and a clipping box inside it that is carried along by
    // the scroll: the shape a document takes on every frame of a glide.
    let mut clips = ClipTable::rooted();
    let port = clips.only(ClipLink::rect(rect(0.0, 0.0, 300.0, 200.0)));
    let inner = rect(10.0, 40.0, 120.0, 60.0);
    let settled = clips.push(port, ClipLink::rect(inner));
    let before = clips.len();

    let mut carried = Size::new(DevicePx(0.0), DevicePx(0.0));
    for _ in 0..1_000 {
        clips.begin_frame();
        carried = Size::new(carried.width, DevicePx(carried.height.0 - 0.5));
        let again = clips.push_shifted(port, ClipLink::rect(inner.translate(carried)), carried);
        assert_eq!(again, settled, "the same clipping box is the same chain");
    }

    assert_eq!(
        clips.len(),
        before,
        "and a thousand notches interned nothing"
    );
    assert_eq!(
        clips.links(settled).last().copied(),
        Some(ClipLink::rect(inner.translate(carried))),
        "while the chain applies the box where it ended up"
    );

    // The other half, which is what makes the first half mean anything: a chain that says nothing
    // about how far it has been carried is a new chain at every position, and the table gains one
    // entry per notch for as long as the scroll runs.
    let mut told_nothing = ClipTable::rooted();
    let port = told_nothing.only(ClipLink::rect(rect(0.0, 0.0, 300.0, 200.0)));
    let before = told_nothing.len();
    let mut carried = Size::new(DevicePx(0.0), DevicePx(0.0));
    for _ in 0..1_000 {
        told_nothing.begin_frame();
        carried = Size::new(carried.width, DevicePx(carried.height.0 - 0.5));
        told_nothing.push(port, ClipLink::rect(inner.translate(carried)));
    }
    assert_eq!(told_nothing.len(), before + 1_000);
}

#[test]
fn a_chain_carried_somewhere_else_is_a_chain_of_its_own() {
    // The other half: naming a chain by its unscrolled rectangle must not merge two boxes that are
    // genuinely different rectangles of the document.
    let mut clips = ClipTable::rooted();
    let carried = Size::new(DevicePx(0.0), DevicePx(-30.0));
    let first = clips.push_shifted(
        ClipId::ROOT,
        ClipLink::rect(rect(0.0, 0.0, 10.0, 10.0)),
        carried,
    );
    let second = clips.push_shifted(
        ClipId::ROOT,
        ClipLink::rect(rect(0.0, 30.0, 10.0, 10.0)),
        carried,
    );
    assert_ne!(first, second);
    assert_eq!(clips.bounds(first), rect(0.0, 0.0, 10.0, 10.0));
    assert_eq!(clips.bounds(second), rect(0.0, 30.0, 10.0, 10.0));
}

/// The defect behind a dialog's field losing its letters: a clip measured inside a transformed
/// subtree, applied where the subtree was laid out instead of where it is drawn.
#[test]
fn a_link_in_a_moved_space_resolves_where_its_box_is_drawn() {
    use zgui_geom::{Corners, Matrix4};

    use crate::spatial::{OwnSpace, PropertyOwner, SpatialTree};

    let mut spaces = SpatialTree::with_viewport();
    let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
    let pulled = Matrix4::translation(-40.0, -20.0, 0.0);
    let space = spaces.space_of(
        spaces.viewport(),
        owner,
        OwnSpace::of(Some(pulled), None, false),
    );

    let mut clips = ClipTable::rooted();
    let field = clips.only(ClipLink::RoundedRect {
        rect: rect(460.0, 310.0, 63.0, 34.0),
        radii: Corners::uniform(Vec2::splat(DevicePx(0.0))),
        space,
    });

    // Read by name, the link stays where it was interned; that is what the insert cull compares
    // against a primitive's own recorded bounds, which are in the same coordinates.
    assert_eq!(clips.bounds(field), rect(460.0, 310.0, 63.0, 34.0));

    // Read against the frame's matrices, it is where the letters are actually drawn. Without this
    // the shader tests device pixels against a rectangle the transform moved out from under them,
    // and everything inside the field is clipped to nothing.
    assert_eq!(
        clips.bounds_placed(field, &|id| spaces.resolve(id)),
        rect(420.0, 290.0, 63.0, 34.0),
    );
}

/// A scaled space scales the rounded test's radii with its rectangle, so the curve content is cut
/// to is the curve being drawn.
#[test]
fn a_link_in_a_scaled_space_scales_its_radii() {
    use zgui_geom::{Corners, Matrix4};

    use crate::spatial::{OwnSpace, PropertyOwner, SpatialTree};

    let mut spaces = SpatialTree::with_viewport();
    let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
    let doubled = Matrix4::scale(2.0, 2.0, 1.0);
    let space = spaces.space_of(
        spaces.viewport(),
        owner,
        OwnSpace::of(Some(doubled), None, false),
    );

    let mut clips = ClipTable::rooted();
    let card = clips.only(ClipLink::RoundedRect {
        rect: rect(10.0, 10.0, 50.0, 30.0),
        radii: Corners::uniform(Vec2::splat(DevicePx(4.0))),
        space,
    });

    let resolved = clips.resolve_placed(card, &|id| spaces.resolve(id));
    assert_eq!(resolved.aabb, [20.0, 20.0, 100.0, 60.0]);
    assert_eq!(resolved.rounded_count, 1);
    assert_eq!(resolved.rounded[0].rect, [20.0, 20.0, 100.0, 60.0]);
    assert_eq!(resolved.rounded[0].radii, [8.0; 8]);
}
