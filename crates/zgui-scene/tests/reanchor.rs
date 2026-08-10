//! What replaying a moved rectangle costs, and what it records.
//!
//! Two claims, and the second is the one that decides between this mechanism and the obvious
//! alternative. A paint that is read at a point has to be told how far its rectangle came, and the
//! counter that says so must fire when one moves and stay silent when none does. And the table the
//! paints live in must be exactly as long after a scroll as before it: re-interning the moved paint
//! would answer the first claim and mint an entry per gradient per offset while doing it.
//!
//! The counter block is process-wide, so each test here takes its turn on it.

use zgui_bits::DamageSet;
use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation};
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_profile::counter::exclusive;
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
use zgui_scene::{ChunkPrims, GradientKind, Paint, PaintRef, Quad, Scene};

/// A device rectangle.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// A displacement down the surface.
fn down(by: f32) -> Size<DevicePx, Device> {
    Size::new(DevicePx(0.0), DevicePx(by))
}

/// A ramp resolved against a box whose top edge is at `top`.
fn ramp(scene: &mut Scene, top: f32) -> PaintRef {
    scene.paints.add(Paint::Gradient {
        kind: GradientKind::Linear {
            start: Point::new(DevicePx(16.0), DevicePx(top)),
            end: Point::new(DevicePx(16.0), DevicePx(top + 64.0)),
        },
        stops: [
            GradientStop::new(0.0, Color::srgb(0.0, 0.0, 1.0, 1.0)),
            GradientStop::new(1.0, Color::srgb(1.0, 1.0, 0.0, 1.0)),
        ]
        .into_iter()
        .collect(),
        space: ColorSpace::Srgb,
        hue: HueInterpolation::Shorter,
        repeating: false,
    })
}

/// A first frame drawing one gradient-filled box and one flat one, and the chunk it recorded.
///
/// The flat box is not decoration: a solid colour is the same everywhere, so moving it re-anchors
/// nothing, and a counter that could not tell the two apart would read two here.
fn painted(gradients: bool) -> (Scene, ChunkPrims) {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(320, 320));
    let flat = PaintRef::solid(scene.paints.solid(Color::srgb(0.2, 0.2, 0.2, 1.0)));
    let fill = if gradients {
        ramp(&mut scene, 16.0)
    } else {
        flat
    };
    scene.push_quad(Quad::filled(rect(16.0, 16.0, 96.0, 64.0), fill));
    scene.push_quad(Quad::filled(rect(16.0, 96.0, 96.0, 64.0), flat));
    let mut recorded = ChunkPrims::default();
    scene.extract_chunk(0..scene.ops().len() as u32, &mut recorded);
    scene.finish(&DamageSet::full());
    (scene, recorded)
}

#[test]
fn a_gradient_carried_to_a_new_position_is_reanchored() {
    let _turn = exclusive();
    counter::reset();
    let (mut scene, recorded) = painted(true);

    scene.begin_frame(Size::new(320, 320));
    let replayed = scene.replay_chunk(&recorded, down(24.0), 0);
    scene.finish(&DamageSet::full());

    assert_eq!(replayed.len(), 2, "both boxes were carried forward");
    assert_eq!(
        scene.primitives.quads[0].paint_origin,
        [0.0, 24.0],
        "the ramp's box says how far it came, so the sampler can undo it"
    );
    if !COUNTERS_ENABLED {
        return;
    }
    assert_eq!(
        counter::get(Counter::PaintsReanchored),
        1,
        "one of the two boxes is filled with a ramp and the other with a flat colour"
    );
}

#[test]
fn a_document_whose_gradients_stand_still_reanchors_nothing() {
    let _turn = exclusive();
    counter::reset();

    // A document full of ramps, replayed exactly where it was.
    let (mut scene, recorded) = painted(true);
    scene.begin_frame(Size::new(320, 320));
    scene.replay_chunk(&recorded, down(0.0), 0);
    scene.finish(&DamageSet::full());

    // And a document that moves, with no ramp in it to move.
    let (mut flat, recorded) = painted(false);
    flat.begin_frame(Size::new(320, 320));
    flat.replay_chunk(&recorded, down(24.0), 0);
    flat.finish(&DamageSet::full());

    if !COUNTERS_ENABLED {
        return;
    }
    assert_eq!(
        counter::get(Counter::PaintsReanchored),
        0,
        "nothing that is read at a point changed position, so nothing was re-anchored"
    );
}

#[test]
fn three_hundred_scroll_steps_intern_no_paint() {
    let _turn = exclusive();
    counter::reset();
    let (mut scene, mut recorded) = painted(true);
    let mut scratch = ChunkPrims::default();
    let interned = scene.paints.len();
    // The count is published at the start of a frame, because what it answers is what the frame
    // inherited, so the first reading worth comparing is the one the first replayed frame opened
    // with.
    let mut live = None;

    for step in 0..300 {
        scene.begin_frame(Size::new(320, 320));
        let inherited = counter::get(Counter::PaintEntriesLive);
        assert_eq!(
            *live.get_or_insert(inherited),
            inherited,
            "step {step} opened with a longer paint table than the step before it"
        );
        let range = scene.replay_chunk(&recorded, down(1.0), 0);
        scene.extract_chunk(range, &mut scratch);
        core::mem::swap(&mut recorded, &mut scratch);
        scene.finish(&DamageSet::full());
        assert_eq!(
            scene.paints.len(),
            interned,
            "step {step} interned a paint, which is one entry per offset and the leak this \
             mechanism exists to avoid"
        );
    }

    assert_eq!(
        scene.primitives.quads[0].paint_origin,
        [0.0, 300.0],
        "three hundred steps of one pixel accumulate into one displacement"
    );
    if !COUNTERS_ENABLED {
        return;
    }
    assert_eq!(
        live,
        Some(interned as u64),
        "the paint table is exactly as long across the whole script as it was after the one frame \
         that painted the document"
    );
}
