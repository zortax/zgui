//! What the two paint tables promise, and why they are two.

use smallvec::smallvec;
use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation};
use zgui_geom::{Device, DevicePx, Point};

use crate::paint::{GradientKind, Paint, PaintKind, PaintTable, TextPaint, TextPaintTable};

/// A point in device space.
fn point(x: f32, y: f32) -> Point<DevicePx, Device> {
    Point::new(DevicePx(x), DevicePx(y))
}

/// A three-stop ramp in Oklch, which is the shape the two-stop reference model cannot hold.
fn oklch_ramp() -> Paint {
    Paint::Gradient {
        kind: GradientKind::Conic {
            center: point(50.0, 50.0),
            from_angle: 0.5,
        },
        stops: smallvec![
            GradientStop {
                offset: 0.0,
                color: Color::srgb(1.0, 0.0, 0.0, 1.0)
            },
            GradientStop {
                offset: 0.5,
                color: Color::srgb(0.0, 1.0, 0.0, 1.0)
            },
            GradientStop {
                offset: 1.0,
                color: Color::srgb(0.0, 0.0, 1.0, 1.0)
            },
        ],
        space: ColorSpace::Oklch,
        hue: HueInterpolation::Longer,
        repeating: true,
    }
}

#[test]
fn a_fifty_stop_gradient_costs_a_primitive_what_a_colour_does() {
    let mut paints = PaintTable::new();
    let solid = paints.add(Paint::Solid(Color::srgb(1.0, 0.0, 0.0, 1.0)));
    let ramp = paints.add(oklch_ramp());

    assert_eq!(size_of_val(&solid), size_of_val(&ramp));
    assert_eq!(size_of_val(&solid), 8);
    assert_eq!(solid.kind, PaintKind::Solid as u32);
    assert_eq!(ramp.kind, PaintKind::Gradient as u32);
}

#[test]
fn the_same_paint_interns_once_and_a_different_one_does_not() {
    let mut paints = PaintTable::new();
    let first = paints.intern(oklch_ramp());
    let again = paints.intern(oklch_ramp());
    assert_eq!(first, again);

    let mut different = oklch_ramp();
    if let Paint::Gradient { hue, .. } = &mut different {
        *hue = HueInterpolation::Shorter;
    }
    assert_ne!(paints.intern(different), first);
    assert_eq!(paints.len(), 2);
}

#[test]
fn the_interpolation_space_is_part_of_the_content() {
    let mut paints = PaintTable::new();
    let oklch = paints.intern(oklch_ramp());
    let mut srgb = oklch_ramp();
    if let Paint::Gradient { space, .. } = &mut srgb {
        *space = ColorSpace::Srgb;
    }
    assert_ne!(paints.intern(srgb), oklch);
}

#[test]
fn a_reference_to_nothing_names_nothing() {
    let mut paints = PaintTable::new();
    let id = paints.solid(Color::srgb(0.0, 0.0, 0.0, 1.0));
    assert_eq!(paints.reference(id).id(), Some(id));
    assert!(crate::PaintRef::NONE.is_none());
    assert_eq!(crate::PaintRef::NONE.id(), None);
}

#[test]
fn two_runs_share_a_brush_when_they_share_a_cascade_result_and_not_when_they_merely_match() {
    let mut brushes = TextPaintTable::new();
    let black = TextPaint::new(Color::srgb(0.0, 0.0, 0.0, 1.0));

    let themed = brushes.slot_for(1, || black);
    let literal = brushes.slot_for(2, || black);
    assert_ne!(
        themed, literal,
        "identical colours from different cascade results must not share a slot"
    );
    assert_eq!(brushes.slot_for(1, || unreachable!()), themed);

    // A theme change re-colours the themed run and leaves the literal one alone.
    brushes.set(themed, TextPaint::new(Color::srgb(1.0, 1.0, 1.0, 1.0)));
    assert_eq!(brushes.get(themed).unwrap().color, [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(brushes.get(literal).unwrap().color, [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn recolouring_rewrites_every_brush_in_place() {
    let mut brushes = TextPaintTable::new();
    let first = brushes.slot_for(1, || TextPaint::new(Color::srgb(0.0, 0.0, 0.0, 1.0)));
    let second = brushes.slot_for(2, || TextPaint::new(Color::srgb(0.0, 0.0, 0.0, 1.0)));

    brushes.recolour(|_, _| TextPaint::new(Color::srgb(0.5, 0.5, 0.5, 1.0)));
    assert_eq!(brushes.get(first).unwrap().color[0], 0.5);
    assert_eq!(brushes.get(second).unwrap().color[0], 0.5);
    assert_eq!(brushes.len(), 2);
}

#[test]
fn a_slot_is_released_only_when_no_key_resolves_to_it() {
    let mut brushes = TextPaintTable::new();
    let black = TextPaint::new(Color::srgb(0.0, 0.0, 0.0, 1.0));
    let slot = brushes.slot_for(1, || black);
    brushes.alias(2, slot);

    brushes.forget(1);
    assert!(!brushes.release(slot), "the alias still resolves to it");
    brushes.forget(2);
    assert!(brushes.release(slot));
    assert!(!brushes.release(slot), "a released slot is released once");
    assert_eq!(brushes.in_use(), 0);

    brushes.alias(3, slot);
    assert_eq!(
        brushes.keys(),
        0,
        "a key cannot be pointed at a released slot"
    );
}

#[test]
fn a_claim_takes_a_released_slot_before_the_table_grows() {
    let mut brushes = TextPaintTable::new();
    let black = TextPaint::new(Color::srgb(0.0, 0.0, 0.0, 1.0));
    let white = TextPaint::new(Color::srgb(1.0, 1.0, 1.0, 1.0));
    let first = brushes.slot_for(1, || black);
    let second = brushes.slot_for(2, || black);
    brushes.forget(1);
    assert!(brushes.release(first));
    let claims = brushes.claims();

    let again = brushes.slot_for(3, || white);
    assert_eq!(again, first);
    assert_eq!(brushes.len(), 2, "the table did not grow");
    assert_eq!(brushes.in_use(), 2);
    assert_eq!(
        brushes.claims(),
        claims + 1,
        "a copy compared by length alone would miss this claim"
    );
    assert_eq!(
        brushes.get(again),
        Some(&white),
        "the claim wrote its own brush"
    );
    assert_eq!(brushes.get(second), Some(&black));
}
