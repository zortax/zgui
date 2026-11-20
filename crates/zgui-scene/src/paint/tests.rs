//! What the two paint tables promise, and why they are two.

use smallvec::smallvec;
use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation};
use zgui_geom::{Device, DevicePx, Point};

use crate::paint::{GradientKind, Paint, PaintKind, PaintTable};

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

/// The background of the box `packed` names.
fn background_of(packed: u64) -> crate::PaintOwner {
    crate::PaintOwner::new(
        crate::PropertyOwner::new(packed).expect("not the empty word"),
        crate::PaintRole::Background,
    )
}

#[test]
fn an_owned_entry_keeps_its_slot_and_its_hash_while_its_colour_moves() {
    let mut paints = PaintTable::new();
    let red = Color::srgb(1.0, 0.0, 0.0, 1.0);
    let grey = Color::srgb(0.5, 0.5, 0.5, 1.0);
    let first = paints.owned(background_of(7), Paint::Solid(red));
    let hash = paints.content_hash(first.id().expect("a slot"));
    let version = paints.version();

    let again = paints.owned(background_of(7), Paint::Solid(grey));
    assert_eq!(again, first, "the same box paint is the same slot");
    assert_eq!(
        paints.get(first.id().expect("a slot")),
        Some(&Paint::Solid(grey))
    );
    assert_eq!(paints.content_hash(first.id().expect("a slot")), hash);
    let mut changed = Vec::new();
    assert_eq!(
        paints.changes_since(version, &mut changed),
        crate::ChangeCoverage::Delta
    );
    assert_eq!(
        changed,
        vec![first.id().expect("a slot")],
        "the rewrite is journaled"
    );
}

#[test]
fn owned_and_interned_entries_never_share_a_slot() {
    let mut paints = PaintTable::new();
    let red = Color::srgb(1.0, 0.0, 0.0, 1.0);
    let shared = paints.add(Paint::Solid(red));
    let owned = paints.owned(background_of(7), Paint::Solid(red));
    let other = paints.owned(background_of(8), Paint::Solid(red));
    assert_ne!(shared, owned);
    assert_ne!(owned, other, "two boxes own two slots");
    assert_eq!(paints.add(Paint::Solid(red)), shared);
}

#[test]
fn a_rewrite_keeps_the_family_a_primitive_names() {
    let mut paints = PaintTable::new();
    let red = Color::srgb(1.0, 0.0, 0.0, 1.0);
    assert_eq!(
        paints.rewrite(background_of(7), Paint::Solid(red)),
        crate::PaintRewrite::Absent
    );
    let slot = paints.owned(background_of(7), Paint::Solid(red));
    assert_eq!(
        paints.rewrite(background_of(7), Paint::Sides([red; 4])),
        crate::PaintRewrite::OtherKind
    );
    assert_eq!(
        paints.get(slot.id().expect("a slot")),
        Some(&Paint::Solid(red))
    );
    let grey = Color::srgb(0.5, 0.5, 0.5, 1.0);
    assert_eq!(
        paints.rewrite(background_of(7), Paint::Solid(grey)),
        crate::PaintRewrite::Written
    );
    assert_eq!(
        paints.get(slot.id().expect("a slot")),
        Some(&Paint::Solid(grey))
    );
}

#[test]
fn a_held_owned_entry_survives_both_sweeps_and_goes_once_released() {
    let mut paints = PaintTable::new();
    let slot = paints
        .owned(
            background_of(7),
            Paint::Solid(Color::srgb(1.0, 0.0, 0.0, 1.0)),
        )
        .id()
        .expect("a slot");
    assert!(paints.retain(slot));
    for _ in 0..300 {
        paints.begin_frame();
    }
    assert_eq!(paints.evict_unreachable(128, usize::MAX), 0);
    assert_eq!(paints.evict_least_recently_used(), 0);
    assert!(paints.contains(slot));

    assert!(paints.release(slot));
    assert_eq!(paints.evict_unreachable(128, usize::MAX), 1);
    assert_eq!(paints.owned_slot(background_of(7)), None);
}
