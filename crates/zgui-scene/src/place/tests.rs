//! What a transform costs when it is written rather than composed again.

use zgui_bits::DamageSet;
use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size, transformed_bounds};
use zgui_profile::{Counter, counter};

use crate::id::ClipId;
use crate::paint::PaintRef;
use crate::place::band::Travel;
use crate::prim::Quad;
use crate::scene::Scene;
use crate::spatial::{OwnSpace, PropertyOwner, SpatialId};

/// The surface every case here is built over.
fn viewport() -> Size<i32, Device> {
    Size::new(256, 256)
}

/// A box handle's packed form.
fn owner(raw: u64) -> PropertyOwner {
    PropertyOwner::new(raw).expect("a handle is never the empty word")
}

/// A rectangle at the origin of whatever space it is drawn in.
fn card() -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(40.0), DevicePx(20.0)),
    )
}

/// A translation along x.
fn slid(x: f32) -> Matrix4 {
    Matrix4::translation(x, 0.0, 0.0)
}

/// A scene holding one quad under one coordinate system of its own, already finished.
///
/// The finished frame is what a caller running before the next one begins is holding, and it is
/// where the ink a placement damages is read from.
fn drawn(at: f32) -> (Scene, SpatialId) {
    let mut scene = Scene::new();
    scene.begin_frame(viewport());
    let root = scene.spatial.viewport();
    let space = scene
        .spatial
        .space_of(root, owner(2), OwnSpace::of(Some(slid(at)), None, false));
    let fill = PaintRef::solid(
        scene
            .paints
            .solid(zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0)),
    );
    scene.push_quad(
        Quad::filled(card(), fill)
            .clipped(ClipId::ROOT)
            .transformed(space),
    );
    scene.finish(&DamageSet::full());
    (scene, space)
}

/// The region a slide between two offsets visits.
fn travel_between(one: f32, two: f32) -> Travel {
    Travel::over([
        transformed_bounds(&slid(one), card()),
        transformed_bounds(&slid(two), card()),
    ])
}

#[test]
fn a_declared_transform_is_written_and_emits_no_primitive() {
    let _turn = counter::exclusive();
    // The whole phase, in one assertion: what a moved box costs is the write and the damage.
    let (mut scene, space) = drawn(0.0);
    scene.declare_travel(space, travel_between(0.0, 60.0));

    let logged = scene.ops().len();
    let drawn = scene.primitives.len();
    let nodes = scene.spatial.len();
    let placement = scene.apply_place(space, slid(30.0));

    assert!(placement.written);
    assert!(!placement.escaped);
    assert_eq!(
        scene.ops().len(),
        logged,
        "a placement write logged an operation"
    );
    assert_eq!(
        scene.primitives.len(),
        drawn,
        "a placement write emitted a primitive"
    );
    assert_eq!(
        scene.spatial.len(),
        nodes,
        "a placement write named a new coordinate system"
    );
    assert_eq!(scene.spatial.resolve(space), Some(slid(30.0)));
}

#[test]
fn the_damage_covers_where_the_ink_was_and_where_it_is() {
    let _turn = counter::exclusive();
    // A set covering only the arrival leaves the box drawn twice: once where it is, once where the
    // previous frame left it and nothing came back to paint over.
    let (mut scene, space) = drawn(0.0);
    scene.declare_travel(space, travel_between(0.0, 60.0));
    let placement = scene.apply_place(space, slid(60.0));

    let was = crate::place::ink::whole(transformed_bounds(&slid(0.0), card()));
    let is = crate::place::ink::whole(transformed_bounds(&slid(60.0), card()));
    assert!(placement.damage.contains(was), "{:?}", placement.damage);
    assert!(placement.damage.contains(is), "{:?}", placement.damage);
}

#[test]
fn a_transform_that_did_not_move_damages_nothing() {
    let _turn = counter::exclusive();
    // Every frame of a transform whose animation is holding its last keyframe. Damaging
    // unconditionally would redraw the box at the refresh rate for the rest of the window's life.
    let (mut scene, space) = drawn(12.0);
    scene.declare_travel(space, travel_between(0.0, 60.0));
    let placement = scene.apply_place(space, slid(12.0));

    assert!(placement.written);
    assert!(placement.damage.is_empty());
}

#[test]
fn an_interactive_transform_is_not_eligible() {
    // A drag has no keyframes and therefore no region to be ordered against, so it is refused the
    // write and composed again. Recorded as a refusal rather than left as an omission.
    let _turn = counter::exclusive();
    let (mut scene, space) = drawn(0.0);
    counter::reset();
    let placement = scene.apply_place(space, slid(30.0));

    assert!(
        !placement.written,
        "a transform with no declared travel was written"
    );
    assert!(
        !placement.escaped,
        "nothing was declared, so nothing was left"
    );
    assert_eq!(counter::get(Counter::PlaceWritesWithReemit), 1);
    assert_eq!(counter::get(Counter::PlaceWritesWithoutReemit), 0);
    assert_eq!(counter::get(Counter::OrderBandEscapes), 0);
    assert_eq!(
        scene.spatial.resolve(space),
        Some(slid(0.0)),
        "a refused placement moved the box anyway"
    );
}

#[test]
fn a_transform_sent_outside_its_declared_travel_escapes() {
    // The non-vacuity half of the escape counter, and the ordering hazard being caught: the box was
    // ordered against the region it declared, so an order it holds outside that region says nothing
    // about the neighbours it now covers.
    let _turn = counter::exclusive();
    let (mut scene, space) = drawn(0.0);
    scene.declare_travel(space, travel_between(0.0, 60.0));
    counter::reset();
    let placement = scene.apply_place(space, slid(200.0));

    assert!(!placement.written);
    assert!(placement.escaped);
    assert_eq!(counter::get(Counter::OrderBandEscapes), 1);
    assert_eq!(counter::get(Counter::PlaceWritesWithReemit), 1);
    assert_eq!(
        scene.spatial.resolve(space),
        Some(slid(0.0)),
        "an escaping placement moved the box instead of asking to be composed"
    );
}

#[test]
fn every_step_inside_the_declared_travel_is_written() {
    // The other half of the same counter: an animation that stays where it said it would go leaves
    // the escape count where it found it, over every frame of the movement.
    let _turn = counter::exclusive();
    let (mut scene, space) = drawn(0.0);
    scene.declare_travel(space, travel_between(0.0, 120.0));
    counter::reset();
    for step in 1..=120 {
        let placement = scene.apply_place(space, slid(step as f32));
        assert!(placement.written, "step {step} was refused");
    }
    assert_eq!(counter::get(Counter::OrderBandEscapes), 0);
    assert_eq!(counter::get(Counter::PlaceWritesWithReemit), 0);
    assert_eq!(counter::get(Counter::PlaceWritesWithoutReemit), 120);
    assert_eq!(
        scene.spatial.len(),
        2,
        "a hundred and twenty frames of one animation named a hundred and twenty spaces"
    );
}

#[test]
fn a_descendant_of_a_moved_coordinate_system_is_damaged_with_it() {
    let _turn = counter::exclusive();
    // The reason the ink is taken over the whole subtree rather than over the node's own
    // primitives: a card that slides carries its label, and a set covering only the card leaves the
    // label's old pixels standing.
    let mut scene = Scene::new();
    scene.begin_frame(viewport());
    let root = scene.spatial.viewport();
    let outer = scene
        .spatial
        .space_of(root, owner(2), OwnSpace::of(Some(slid(0.0)), None, false));
    let inner = scene.spatial.space_of(
        outer,
        owner(3),
        OwnSpace::of(Some(slid(100.0)), None, false),
    );
    let fill = PaintRef::solid(
        scene
            .paints
            .solid(zgui_color::Color::srgb(0.0, 1.0, 0.0, 1.0)),
    );
    scene.push_quad(
        Quad::filled(card(), fill)
            .clipped(ClipId::ROOT)
            .transformed(inner),
    );
    scene.finish(&DamageSet::full());

    scene.declare_travel(
        outer,
        Travel::over([
            transformed_bounds(&slid(100.0), card()),
            transformed_bounds(&slid(140.0), card()),
        ]),
    );
    let placement = scene.apply_place(outer, slid(40.0));
    assert!(placement.written);
    let label_was = crate::place::ink::whole(transformed_bounds(&slid(100.0), card()));
    let label_is = crate::place::ink::whole(transformed_bounds(&slid(140.0), card()));
    assert!(
        placement.damage.contains(label_was),
        "{:?}",
        placement.damage
    );
    assert!(
        placement.damage.contains(label_is),
        "{:?}",
        placement.damage
    );
}
