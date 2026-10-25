//! The kept device matrices and placements follow every write they depend on, and clip culls
//! follow every rewrite of a link.

use std::borrow::Cow;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size};

use crate::clip::ClipLink;
use crate::id::ClipId;
use crate::paint::PaintRef;
use crate::prim::Quad;
use crate::scene::Scene;
use crate::spatial::{OwnSpace, PropertyOwner, SpatialId};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

fn scene() -> (Scene, PaintRef) {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(400, 400));
    let id = scene.paints.solid(Color::srgb(1.0, 0.0, 0.0, 1.0));
    (scene, PaintRef::solid(id))
}

fn owner(word: u64) -> PropertyOwner {
    PropertyOwner::new(word).expect("a handle is never the empty word")
}

/// A space of its own under the viewport, moved down by `y`.
fn space(scene: &mut Scene, y: f32) -> SpatialId {
    let viewport = scene.spatial.viewport();
    let own = OwnSpace::of(Some(Matrix4::translation(0.0, y, 0.0)), None, false);
    scene.spatial.space_of(viewport, owner(2), own)
}

fn quad_in(space: SpatialId, at: Rect<DevicePx, Device>, fill: PaintRef) -> Quad {
    let mut quad = Quad::filled(at, fill);
    quad.transform = space.index();
    quad
}

#[test]
fn a_space_moved_between_pushes_orders_the_next_push_where_it_now_lands() {
    let (mut scene, fill) = scene();
    let below = space(&mut scene, 200.0);
    assert_eq!(
        scene.push_quad(Quad::filled(rect(100.0, 0.0, 30.0, 30.0), fill)),
        Some(1)
    );
    // Drawn at (300, 200): overlaps nothing, and keeps the space's matrix.
    assert_eq!(
        scene.push_quad(quad_in(below, rect(300.0, 0.0, 30.0, 30.0), fill)),
        Some(1)
    );

    assert!(scene.spatial.place(below, Matrix4::IDENTITY));
    // Drawn at (100, 0) now, over the first quad. Through the kept matrix it would land at
    // (100, 200) and take order 1.
    assert_eq!(
        scene.push_quad(quad_in(below, rect(100.0, 0.0, 30.0, 30.0), fill)),
        Some(2)
    );
}

#[test]
fn a_named_clip_rewritten_in_place_culls_the_next_push_against_its_new_rectangle() {
    let (mut scene, fill) = scene();
    let port = |scene: &mut Scene, y: f32| {
        scene.clips.push_named(
            ClipId::ROOT,
            ClipLink::rect(rect(0.0, y, 400.0, 100.0)),
            Size::ZERO,
            owner(7),
        )
    };
    let clip = port(&mut scene, 0.0);
    let row = Quad::filled(rect(0.0, 10.0, 50.0, 20.0), fill).clipped(clip);
    assert!(scene.push_quad(row).is_some());

    assert_eq!(
        port(&mut scene, 200.0),
        clip,
        "a resize keeps the chain's slot"
    );
    assert!(
        scene.push_quad(row).is_none(),
        "the rewritten rectangle admits none of it"
    );
    let moved = Quad::filled(rect(0.0, 210.0, 50.0, 20.0), fill).clipped(clip);
    assert!(scene.push_quad(moved).is_some());
}

#[test]
fn a_shifted_clip_re_interned_in_place_culls_the_next_push_against_its_new_rectangle() {
    // The route a minted clip takes when its chunk replays somewhere else.
    let (mut scene, fill) = scene();
    let clip = scene.clips.push_shifted(
        ClipId::ROOT,
        ClipLink::rect(rect(0.0, 0.0, 400.0, 100.0)),
        Size::ZERO,
    );
    let row = Quad::filled(rect(0.0, 10.0, 50.0, 20.0), fill).clipped(clip);
    assert!(scene.push_quad(row).is_some());

    let shifted = scene.clips.push_shifted(
        ClipId::ROOT,
        ClipLink::rect(rect(0.0, 200.0, 400.0, 100.0)),
        Size::new(DevicePx(0.0), DevicePx(200.0)),
    );
    assert_eq!(shifted, clip, "the same settled chain");
    assert!(
        scene.push_quad(row).is_none(),
        "the shifted rectangle admits none of it"
    );
}

/// Pushes a vector item, the reader that makes a finish keep the placements.
fn drawing(scene: &mut Scene, fill: PaintRef) {
    let path = kurbo::Shape::to_path(&kurbo::Rect::new(0.0, 0.0, 20.0, 20.0), 0.1);
    scene.push_vector(crate::VectorItem::filled(
        crate::VectorId(1),
        std::sync::Arc::new(path),
        fill,
    ));
}

#[test]
fn the_finished_placements_are_kept_until_the_tree_is_written() {
    let (mut scene, fill) = scene();
    let below = space(&mut scene, 200.0);
    scene.push_quad(quad_in(below, rect(0.0, 0.0, 30.0, 30.0), fill));
    drawing(&mut scene, fill);
    scene.finish(&DamageSet::full());
    let kept = scene.placements();
    assert!(matches!(kept, Cow::Borrowed(_)), "the finish resolved them");
    assert_eq!(
        kept.get(below),
        Some(&Matrix4::translation(0.0, 200.0, 0.0))
    );

    assert!(scene.spatial.place(below, Matrix4::IDENTITY));
    let fresh = scene.placements();
    assert!(
        matches!(fresh, Cow::Owned(_)),
        "a write makes the kept copy stale"
    );
    assert_eq!(fresh.get(below), Some(&Matrix4::IDENTITY));

    // A frame with nothing that reads them leaves the stale copy, and the answer stays fresh.
    scene.begin_frame(Size::new(400, 400));
    scene.finish(&DamageSet::full());
    let unread = scene.placements();
    assert!(matches!(unread, Cow::Owned(_)));
    assert_eq!(unread.get(below), Some(&Matrix4::IDENTITY));

    scene.begin_frame(Size::new(400, 400));
    drawing(&mut scene, fill);
    scene.finish(&DamageSet::full());
    let next = scene.placements();
    assert!(matches!(next, Cow::Borrowed(_)));
    assert_eq!(next.get(below), Some(&Matrix4::IDENTITY));
}
