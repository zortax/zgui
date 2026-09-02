//! What a corner shape draws, on a real device.
//!
//! A corner is a superellipse quadrant, and the exponent is the whole of the difference between
//! the shapes CSS names. What is asserted here is that the exponent reaches the pixels, that it
//! reaches every one of the four things a box's corner decides — background, border, shadow and
//! the clip it gives its children — and above all that the ellipse still draws what it always
//! drew.

mod support;

use zgui_bits::DamageSet;
use zgui_geom::{Corners, DevicePx, Size, Vec2};
use zgui_scene::{ClipLink, CornerShape, Quad, Scene, Shadow};

use support::{SIDE, opaque, plain_renderer, present, rect};

/// Uniform radii of `radius` on every corner.
fn radii(radius: f32) -> Corners<Vec2<DevicePx>> {
    let corner = Vec2::new(DevicePx(radius), DevicePx(radius));
    Corners {
        top_left: corner,
        top_right: corner,
        bottom_right: corner,
        bottom_left: corner,
    }
}

fn scene() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene
}

/// A black box of `side`, cornered at `radius`, cut to `shape`.
fn boxed(shape: CornerShape, radius: f32, side: f32) -> Scene {
    let mut scene = scene();
    let fill = scene.paints.add(zgui_scene::Paint::Solid(opaque(0, 0, 0)));
    scene.push_quad(
        Quad::filled(rect(0.0, 0.0, side, side), fill)
            .with_radii(radii(radius))
            .with_corner_shape(shape),
    );
    scene.finish(&DamageSet::full());
    scene
}

/// The one thing that must not change: an exponent of two is the ellipse, drawn exactly as it was
/// drawn before shapes existed. Anything else moves every rounded box in every document.
#[test]
fn the_round_shape_draws_what_a_corner_radius_always_drew() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut plain = scene();
    let fill = plain.paints.add(zgui_scene::Paint::Solid(opaque(0, 0, 0)));
    // Built without ever naming a shape, which is what every existing caller does.
    plain.push_quad(Quad::filled(rect(0.0, 0.0, 60.0, 60.0), fill).with_radii(radii(20.0)));
    plain.finish(&DamageSet::full());
    let before = present(&mut renderer, &plain);

    let named = boxed(CornerShape::ROUND, 20.0, 60.0);
    let after = present(&mut renderer, &named);
    assert_eq!(
        after.max_difference(&before),
        0,
        "naming the ellipse draws the same pixels as not naming anything"
    );
}

/// A squircle is fuller than a circle: the point the circle cuts away is inside the squircle.
#[test]
fn a_squircle_covers_the_corner_a_circle_cuts_away() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let round = present(&mut renderer, &boxed(CornerShape::ROUND, 24.0, 60.0));
    let squircle = present(&mut renderer, &boxed(CornerShape::SQUIRCLE, 24.0, 60.0));

    // Just inside the radius box's own corner, where a circle of twenty-four has fallen away.
    assert_eq!(round.rgba(5, 5)[3], 0, "the circle has cut this away");
    assert_eq!(squircle.rgba(5, 5)[3], 255, "and the squircle has not");
    // Both still fill the middle and both still stop at the box.
    assert_eq!(round.rgba(30, 30)[3], 255);
    assert_eq!(squircle.rgba(30, 30)[3], 255);
    assert_eq!(squircle.rgba(70, 70)[3], 0, "and neither leaves its box");
}

/// A bevel is a straight chamfer, which is the superellipse at one: the corner is a line, so the
/// point half way along it is exactly on the boundary.
#[test]
fn a_bevel_cuts_the_corner_straight() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let bevel = present(&mut renderer, &boxed(CornerShape::BEVEL, 24.0, 60.0));
    // Well inside the chamfer.
    assert_eq!(bevel.rgba(20, 20)[3], 255);
    // Well outside it: the straight cut runs from (0, 24) to (24, 0), so (4, 4) is beyond it.
    assert_eq!(bevel.rgba(4, 4)[3], 0);
    // And a bevel cuts away more than a circle does, which is the whole difference.
    let round = present(&mut renderer, &boxed(CornerShape::ROUND, 24.0, 60.0));
    assert!(
        u32::from(round.rgba(9, 9)[3]) > u32::from(bevel.rgba(9, 9)[3]),
        "the circle is fuller than the chamfer"
    );
}

/// A scoop cuts inwards, so it takes away more than even the chamfer.
#[test]
fn a_scoop_cuts_further_in_than_a_bevel() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let bevel = present(&mut renderer, &boxed(CornerShape::BEVEL, 24.0, 60.0));
    let scoop = present(&mut renderer, &boxed(CornerShape::SCOOP, 24.0, 60.0));
    assert!(
        u32::from(bevel.rgba(16, 16)[3]) > u32::from(scoop.rgba(16, 16)[3]),
        "the scoop is cut inwards past the chamfer"
    );
}

/// A shadow is the box's own shape blurred, so it is cut the same way. A shadow that stayed
/// elliptical under a squircle shows its corners outside the box that casts it.
#[test]
fn a_shadow_is_cut_the_way_the_box_that_casts_it_is() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    // A spread, so the shadow's own shape is larger than the element that casts it: a drop shadow
    // is never painted inside its own element, so a shadow the same size as its box is entirely
    // cut away and says nothing about corners.
    let shadowed = |shape: CornerShape| {
        let mut scene = scene();
        let mut shadow = Shadow::drop_shadow(
            rect(20.0, 20.0, 40.0, 40.0),
            (0.0, 0.0),
            8.0,
            0.0,
            opaque(0, 0, 0),
        );
        shadow.radii = [24.0; 8];
        shadow.element_radii = [16.0; 8];
        shadow = shadow.with_corner_shape(shape);
        scene.push_shadow(shadow);
        scene.finish(&DamageSet::full());
        scene
    };
    let round = present(&mut renderer, &shadowed(CornerShape::ROUND));
    let squircle = present(&mut renderer, &shadowed(CornerShape::SQUIRCLE));

    // Outside the element, and outside a circle of twenty-four about the shadow's own corner.
    assert_eq!(round.rgba(16, 16)[3], 0, "the circle cuts this corner away");
    assert_eq!(
        squircle.rgba(16, 16)[3],
        255,
        "and the squircle's shadow fills it, as its box does"
    );
    // Both still draw the band between the element and the spread.
    assert_eq!(round.rgba(14, 40)[3], 255, "the spread is drawn either way");
}

/// The clip a box gives its children is cut the same way too, or content inside a squircle shows
/// its own corners past the card's.
#[test]
fn a_clip_is_cut_the_way_the_box_that_gives_it_is() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let clipped = |shape: CornerShape| {
        let mut scene = scene();
        let clip = scene.clips.only(ClipLink::shaped(
            rect(0.0, 0.0, 60.0, 60.0),
            radii(24.0),
            shape,
            zgui_scene::SpatialId::VIEWPORT,
        ));
        let fill = scene.paints.add(zgui_scene::Paint::Solid(opaque(0, 0, 0)));
        // A square-cornered child, so everything about the corner is the clip's doing.
        scene.push_quad(Quad::filled(rect(0.0, 0.0, 60.0, 60.0), fill).clipped(clip));
        scene.finish(&DamageSet::full());
        scene
    };
    let round = present(&mut renderer, &clipped(CornerShape::ROUND));
    let squircle = present(&mut renderer, &clipped(CornerShape::SQUIRCLE));
    assert_eq!(round.rgba(5, 5)[3], 0, "the circle clips this away");
    assert_eq!(squircle.rgba(5, 5)[3], 255, "and the squircle keeps it");
}

/// A box with no radii has no corner to cut, whatever shape it names.
#[test]
fn a_square_box_is_the_same_box_whatever_shape_it_names() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let plain = present(&mut renderer, &boxed(CornerShape::ROUND, 0.0, 60.0));
    for shape in [
        CornerShape::SQUIRCLE,
        CornerShape::BEVEL,
        CornerShape::SCOOP,
    ] {
        let named = present(&mut renderer, &boxed(shape, 0.0, 60.0));
        assert_eq!(
            named.max_difference(&plain),
            0,
            "{shape:?} has no corner to cut"
        );
    }
}
