//! What a primitive's name for its coordinate system has to go on meaning.

use zgui_bits::DamageSet;
use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size};

use crate::id::ClipId;
use crate::paint::PaintRef;
use crate::prim::Quad;
use crate::scene::Scene;
use crate::spatial::{OwnSpace, PropertyOwner, SpatialId};

/// The surface every case here is built over.
fn viewport_size() -> Size<i32, Device> {
    Size::new(256, 256)
}

/// A box handle's packed form, standing in for the boxes a document is made of.
fn owner(raw: u64) -> PropertyOwner {
    PropertyOwner::new(raw).expect("a handle is never the empty word")
}

/// A coordinate system moved along x and scrolling like everything else.
fn moved(x: f32) -> Option<OwnSpace> {
    OwnSpace::of(Some(Matrix4::translation(x, 0.0, 0.0)), None, false)
}

/// A small rectangle inside the surface.
fn bounds() -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(64.0), DevicePx(24.0)),
    )
}

/// A scene keeping the names, with a frame begun.
fn scene() -> Scene {
    let mut scene = Scene::new();
    scene.record_spatial_dependencies(true);
    scene.begin_frame(viewport_size());
    scene
}

/// One quad filling [`bounds`], drawn under `space`.
fn quad(scene: &mut Scene, space: SpatialId) -> Quad {
    let fill = PaintRef::solid(
        scene
            .paints
            .solid(zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0)),
    );
    Quad::filled(bounds(), fill)
        .clipped(ClipId::ROOT)
        .transformed(space)
}

#[test]
fn a_frame_drawn_through_the_coordinate_systems_it_established_reports_nothing() {
    let mut scene = scene();
    let viewport = scene.spatial.viewport();
    let card = scene.spatial.space_of(viewport, owner(2), moved(10.0));
    let drawn = quad(&mut scene, card);
    scene.push_quad(drawn).expect("inside the surface");
    let plain = quad(&mut scene, viewport);
    scene.push_quad(plain).expect("inside the surface");
    scene.finish(&DamageSet::full());

    assert_eq!(scene.spatial_faults(), Vec::new());
}

#[test]
fn a_replayed_primitive_whose_slot_changed_hands_is_reported() {
    // The failure the occupancy counter exists for, and the only place in the project it is
    // visible. Everything about the stranger below is chosen so that nothing else could notice: it
    // is handed the departing card's slot, and it is given *the same matrix*, so the primitive
    // resolves to the same place, draws the same pixels and prints the same transcript line. The
    // only thing that is wrong is which box's coordinate system the primitive is being carried by,
    // and a check that asked whether its slot resolves would find that it does.
    let mut scene = scene();
    let viewport = scene.spatial.viewport();
    let card = owner(2);
    let space = scene.spatial.space_of(viewport, card, moved(10.0));
    let drawn = quad(&mut scene, space);
    scene.push_quad(drawn).expect("inside the surface");
    let mut recorded = crate::scene::chunk::ChunkPrims::default();
    scene.extract_chunk(0..scene.ops().len() as u32, &mut recorded);
    assert_eq!(
        scene.spatial_faults(),
        Vec::new(),
        "the frame that pushed it is intact",
    );
    scene.finish(&DamageSet::full());
    // The end of the frame: the card is taken out of the document and gives its node back.
    scene.spatial.release(card);

    scene.begin_frame(viewport_size());
    // The next frame's fragment walk, which is where a released node stops resolving and its slot
    // becomes available again.
    scene.spatial.recycle();
    let stranger = scene.spatial.space_of(viewport, owner(3), moved(10.0));
    assert_eq!(
        stranger.index(),
        space.index(),
        "the slot came back, which is the whole premise",
    );
    assert_ne!(stranger, space);
    scene.replay_chunk(&recorded, Size::new(DevicePx(0.0), DevicePx(0.0)));

    let faults = scene.spatial_faults();
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].named, space);
    assert_eq!(faults[0].holding, Some(stranger));
    assert!(
        scene.spatial.resolve(stranger) == scene.spatial.resolve_at(space.index()),
        "the stranger's matrix is the departed card's matrix, so nothing downstream differs",
    );
}

#[test]
fn a_replayed_primitive_whose_coordinate_system_merely_moved_is_not_reported() {
    // The other half, and the one that decides whether this check is usable at all: a structural
    // name is the same name while the box it belongs to moves, and moving is what the whole
    // representation exists to make cheap. A check that reported movement would fire on every frame
    // of every animation.
    let mut scene = scene();
    let viewport = scene.spatial.viewport();
    let card = owner(2);
    let space = scene.spatial.space_of(viewport, card, moved(10.0));
    let drawn = quad(&mut scene, space);
    scene.push_quad(drawn).expect("inside the surface");
    let mut recorded = crate::scene::chunk::ChunkPrims::default();
    scene.extract_chunk(0..scene.ops().len() as u32, &mut recorded);
    scene.finish(&DamageSet::full());

    scene.begin_frame(viewport_size());
    scene.spatial.recycle();
    assert_eq!(
        scene.spatial.space_of(viewport, card, moved(90.0)),
        space,
        "a tick moves the matrix and keeps the name",
    );
    scene.replay_chunk(&recorded, Size::new(DevicePx(0.0), DevicePx(0.0)));

    assert_eq!(scene.spatial_faults(), Vec::new());
}

#[test]
fn a_scene_that_is_not_recording_has_nothing_to_report() {
    // What every window that did not ask for this pays: no storage, no lookup, and no answer.
    let mut scene = Scene::new();
    scene.record_spatial_dependencies(false);
    scene.begin_frame(viewport_size());
    let viewport = scene.spatial.viewport();
    let card = owner(2);
    let space = scene.spatial.space_of(viewport, card, moved(10.0));
    let drawn = quad(&mut scene, space);
    scene.push_quad(drawn).expect("inside the surface");
    let mut recorded = crate::scene::chunk::ChunkPrims::default();
    scene.extract_chunk(0..scene.ops().len() as u32, &mut recorded);
    scene.finish(&DamageSet::full());
    scene.spatial.release(card);

    scene.begin_frame(viewport_size());
    scene.spatial.recycle();
    scene.spatial.space_of(viewport, owner(3), moved(10.0));
    scene.replay_chunk(&recorded, Size::new(DevicePx(0.0), DevicePx(0.0)));

    assert_eq!(scene.spatial_faults(), Vec::new());
}

#[test]
#[should_panic(expected = "coordinate systems that changed hands")]
fn the_frame_loop_refuses_to_hand_a_faulty_display_list_to_a_renderer() {
    // The wired form of the case above. What follows a finished frame is a renderer filling a dense
    // array of matrices and indexing it with the numbers these primitives carry, and it has no way
    // to know that one of them means something else now.
    let mut scene = scene();
    let viewport = scene.spatial.viewport();
    let card = owner(2);
    let space = scene.spatial.space_of(viewport, card, moved(10.0));
    let drawn = quad(&mut scene, space);
    scene.push_quad(drawn).expect("inside the surface");
    let mut recorded = crate::scene::chunk::ChunkPrims::default();
    scene.extract_chunk(0..scene.ops().len() as u32, &mut recorded);
    scene.finish(&DamageSet::full());
    scene.check_spatial_dependencies();
    scene.spatial.release(card);

    scene.begin_frame(viewport_size());
    scene.spatial.recycle();
    scene.spatial.space_of(viewport, owner(3), moved(10.0));
    scene.replay_chunk(&recorded, Size::new(DevicePx(0.0), DevicePx(0.0)));
    scene.finish(&DamageSet::full());
    scene.check_spatial_dependencies();
}
