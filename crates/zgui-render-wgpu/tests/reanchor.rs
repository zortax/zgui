//! A ramp travels with the rectangle it fills.
//!
//! A paint states its geometry in the coordinates it was resolved against, and a rectangle carried
//! forward to a new position keeps the paint it already had. Without something to reconcile the
//! two, a box that moved samples its ramp where the box used to be — past the end of it, once the
//! box has travelled further than its own height — and goes flat in whichever colour the ramp ends
//! in. These are the pictures that say it does not: one drawn by moving a rectangle, one drawn from
//! scratch at the place it moved to, and no channel of no pixel between them.

mod support;

use zgui_bits::DamageSet;
use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation};
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_render_wgpu::Pixels;
use zgui_scene::prim::BorderStyle;
use zgui_scene::{GradientKind, Paint, PaintRef, Quad, Scene};

use support::{SIDE, plain_renderer, present, rect};

/// The rectangle's left edge and width.
const LEFT: f32 = 16.0;
/// How wide it is.
const WIDTH: f32 = 96.0;
/// How tall it is.
const HEIGHT: f32 = 40.0;
/// Where it is first painted.
const FIRST: f32 = 8.0;
/// How far it travels, which is more than its own height so a clamped ramp runs off the end of
/// itself rather than merely sampling the wrong part of itself.
const STEP: f32 = 64.0;

/// The rectangle, with its top edge at `top`.
fn box_at(top: f32) -> Rect<DevicePx, Device> {
    rect(LEFT, top, WIDTH, HEIGHT)
}

/// A ramp running across a rectangle whose top edge is at `top`, in whichever shape `radial` asks
/// for.
///
/// This is what resolving a gradient against a box does: the ramp's geometry is the box's, in the
/// coordinates the box had when the paint was made.
fn ramp(scene: &mut Scene, top: f32, radial: bool) -> PaintRef {
    let kind = if radial {
        GradientKind::Radial {
            center: Point::new(DevicePx(LEFT + WIDTH * 0.5), DevicePx(top + HEIGHT * 0.5)),
            radius_x: WIDTH * 0.5,
            radius_y: HEIGHT * 0.5,
        }
    } else {
        GradientKind::Linear {
            start: Point::new(DevicePx(LEFT), DevicePx(top)),
            end: Point::new(DevicePx(LEFT + WIDTH), DevicePx(top + HEIGHT)),
        }
    };
    // Three colours far enough apart that sampling the wrong part of the ramp is a difference no
    // rounding could have produced.
    let stops = [
        GradientStop::new(0.0, Color::srgb_u8(126, 227, 255, 255)),
        GradientStop::new(0.55, Color::srgb_u8(47, 107, 255, 255)),
        GradientStop::new(1.0, Color::srgb_u8(209, 139, 255, 255)),
    ];
    scene.paints.add(Paint::Gradient {
        kind,
        stops: stops.into_iter().collect(),
        space: ColorSpace::Srgb,
        hue: HueInterpolation::Shorter,
        repeating: false,
    })
}

/// Which of the three quads under test a scene draws.
#[derive(Clone, Copy)]
enum Arm {
    /// A rectangle filled with a linear ramp.
    Fill,
    /// A rectangle whose border is painted with a ramp and whose middle is not painted at all.
    Stroke,
    /// A rectangle filled with a radial ramp, which is the other shape a background image layer
    /// resolves to.
    Radial,
}

/// Draws `arm`'s quad into `scene` with its top edge at `top`, interning the ramp against it.
fn draw(scene: &mut Scene, arm: Arm, top: f32) {
    let bounds = box_at(top);
    let quad = match arm {
        Arm::Fill => Quad::filled(bounds, ramp(scene, top, false)),
        Arm::Radial => Quad::filled(bounds, ramp(scene, top, true)),
        Arm::Stroke => {
            let stroke = ramp(scene, top, false);
            Quad::filled(bounds, PaintRef::NONE).with_border([10.0; 4], stroke, BorderStyle::Solid)
        }
    };
    scene.push_quad(quad);
}

/// A scene drawing `arm` once, at its travelled position, with nothing carried forward.
fn repainted(arm: Arm) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    draw(&mut scene, arm, FIRST + STEP);
    scene.finish(&DamageSet::full());
    scene
}

/// A scene drawing `arm` where it started, then replaying that frame [`STEP`] further down.
///
/// The second frame emits no primitive of its own and interns no paint: it hands back the range the
/// first frame recorded, which is exactly what a scrolled fragment costs.
fn scrolled(arm: Arm) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    draw(&mut scene, arm, FIRST);
    scene.finish(&DamageSet::full());
    let recorded = 0..scene.ops().len() as u32;
    let interned = scene.paints.len();

    scene.begin_frame(Size::new(SIDE, SIDE));
    let replayed = scene.replay(recorded, Size::new(DevicePx(0.0), DevicePx(STEP)));
    assert_eq!(replayed.len(), 1, "the quad was replayed");
    assert_eq!(
        scene.paints.len(),
        interned,
        "replaying a moved rectangle must not intern a paint: a table that grows once per scroll \
         offset is a leak, and this is the assertion that says this mechanism is not one"
    );
    scene.finish(&DamageSet::full());
    scene
}

/// Whether a picture shows a ramp rather than a flat colour over the rectangle under test.
///
/// The comparison below is worth nothing over a box that is one colour everywhere: two flat boxes
/// agree whatever the sampler does. This is what says the fixture is not that.
fn varies(picture: &Pixels) -> bool {
    let y = (FIRST + STEP + HEIGHT * 0.5) as i32;
    picture.rgba(LEFT as i32 + 6, y) != picture.rgba((LEFT + WIDTH) as i32 - 6, y)
}

/// Draws both scenes of `arm` on one device and returns the two pictures.
fn pictures(arm: Arm) -> Option<(Pixels, Pixels)> {
    let mut renderer = plain_renderer()?;
    let whole = present(&mut renderer, &repainted(arm));
    let moved = present(&mut renderer, &scrolled(arm));
    Some((whole, moved))
}

/// The two pictures, with the fixture's own non-vacuity checked first.
fn assert_identical(arm: Arm, name: &str) {
    let Some((whole, moved)) = pictures(arm) else {
        return;
    };
    assert!(
        varies(&whole),
        "{name}: the rectangle repainted from scratch is one colour across its width, so this \
         comparison could not tell a ramp that travels with its box from one that does not"
    );
    assert_eq!(
        moved.max_difference(&whole),
        0,
        "{name}: a rectangle carried forward to a new position drew differently from the same \
         rectangle painted there from scratch"
    );
}

#[test]
fn a_scrolled_gradient_box_is_identical_to_a_repaint() {
    assert_identical(Arm::Fill, "a linear ramp filling a box");
}

#[test]
fn a_scrolled_gradient_stroke_and_a_scrolled_background_image_are_identical_to_a_repaint() {
    // Both of the arms the fill arm cannot speak for. The stroke is a second paint reference on the
    // same instance, read by a different branch of the shader; the radial ramp is the other shape a
    // background image layer resolves to, and it is centred rather than run along a line, so a
    // displacement moves it in a way a linear ramp's would not show.
    assert_identical(Arm::Stroke, "a ramp painted along a border");
    assert_identical(Arm::Radial, "a radial ramp filling a box");
}
