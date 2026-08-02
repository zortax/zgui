//! What the scene transcript promises: the same scene renders identically, and a different scene
//! does not.

mod support;

use std::path::PathBuf;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Point, Rect, Size};
use zgui_scene::{ClipLink, PaintRef, Quad, Scene};
use zgui_testkit_scene::dump::golden;
use zgui_testkit_scene::transcript;

use crate::support::{kitchen_sink, rect};

/// Where this crate's goldens live.
fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(name)
}

#[test]
fn the_same_scene_renders_byte_identically_a_hundred_times() {
    let first = transcript::of(&kitchen_sink(), &DamageSet::full());
    for _ in 0..100 {
        assert_eq!(
            transcript::of(&kitchen_sink(), &DamageSet::full()),
            first,
            "a transcript that is not byte-stable is not a regression artifact"
        );
    }
}

#[test]
fn the_transcript_it_is_stable_at_is_not_an_empty_one() {
    // The control for the stability test above. An empty rendering is perfectly stable, and a
    // transcript that dropped a primitive kind would be stable while holding a golden green through
    // that kind's every regression.
    let rendered = transcript::of(&kitchen_sink(), &DamageSet::full());
    let text = rendered.as_str();
    for kind in [
        "group_start",
        "shadow",
        "quad",
        "vector",
        "decoration",
        "mono_sprite",
        "subpixel_sprite",
        "color_sprite",
        "external",
        "backdrop",
        "group_end",
    ] {
        assert!(
            text.contains(kind),
            "the transcript never mentions `{kind}`"
        );
    }
    assert!(rendered.line_count() > 15);
}

#[test]
fn every_resolved_field_a_primitive_carries_reaches_the_page() {
    let rendered = transcript::of(&kitchen_sink(), &DamageSet::full());
    let text = rendered.as_str();
    // A paint resolved through the table, not an index.
    assert!(text.contains("solid srgb(0.2, 0.4, 0.6, 1)"));
    // A gradient with its interpolation space and its stops.
    assert!(text.contains("linear from=(0, 0) to=(256, 0)"));
    assert!(text.contains("in oklab"));
    assert!(text.contains("oklch(0.7, 0.1, 320, 1)"));
    // A clip chain by its links, and a mask link by its tile.
    assert!(text.contains("clip=[rect(8, 8, 240, 96) radii="));
    assert!(text.contains("mask mono:0#3"));
    // A transform by its matrix.
    assert!(text.contains("transform=#1 ["));
    // The group's blend, opacity and filters, and the read extent a blur forces.
    assert!(text.contains("opacity=0.5 blend=Multiply/SrcOver filters=[blur(3)]"));
    assert!(text.contains("source=rect(-1, 15, 138, 98)"));
    // Vector geometry, not only its box.
    assert!(text.contains("fill_rule=EvenOdd"));
    assert!(text.contains(" d=\"M"));
    // Border style and dash phase.
    assert!(text.contains("style=dashed"));
    // A decoration's style by name.
    assert!(text.contains("style=wavy"));
}

#[test]
fn a_different_paint_renders_differently() {
    let one = transcript::of(
        &one_quad(Color::srgb(1.0, 0.0, 0.0, 1.0)),
        &DamageSet::full(),
    );
    let other = transcript::of(
        &one_quad(Color::srgb(0.0, 1.0, 0.0, 1.0)),
        &DamageSet::full(),
    );
    assert_ne!(one, other);
}

#[test]
fn a_different_clip_renders_differently() {
    let mut clipped = Scene::new();
    clipped.begin_frame(Size::new(64, 64));
    let fill = PaintRef::solid(clipped.paints.solid(Color::BLACK));
    let clip = clipped
        .clips
        .only(ClipLink::rect(rect(0.0, 0.0, 32.0, 32.0)));
    clipped.push_quad(Quad::filled(rect(0.0, 0.0, 16.0, 16.0), fill).clipped(clip));
    clipped.finish(&DamageSet::full());

    let mut unclipped = Scene::new();
    unclipped.begin_frame(Size::new(64, 64));
    let fill = PaintRef::solid(unclipped.paints.solid(Color::BLACK));
    unclipped.push_quad(Quad::filled(rect(0.0, 0.0, 16.0, 16.0), fill));
    unclipped.finish(&DamageSet::full());

    assert_ne!(
        transcript::of(&clipped, &DamageSet::full()),
        transcript::of(&unclipped, &DamageSet::full())
    );
}

#[test]
fn paint_order_reaches_the_transcript_and_mount_order_does_not() {
    // Two overlapping quads: whichever is pushed second draws above, and the transcript has to say
    // so. This is the property a recording of mount operations structurally cannot see.
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(64, 64));
    let red = PaintRef::solid(scene.paints.solid(Color::srgb(1.0, 0.0, 0.0, 1.0)));
    let blue = PaintRef::solid(scene.paints.solid(Color::srgb(0.0, 0.0, 1.0, 1.0)));
    let below = scene
        .push_quad(Quad::filled(rect(0.0, 0.0, 32.0, 32.0), red))
        .expect("nothing clips it away");
    let above = scene
        .push_quad(Quad::filled(rect(8.0, 8.0, 32.0, 32.0), blue))
        .expect("nothing clips it away");
    assert!(above > below, "the overlapping quad sorts above");
    scene.finish(&DamageSet::full());

    let rendered = transcript::of(&scene, &DamageSet::full());
    let lines: Vec<&str> = rendered
        .lines()
        .filter(|line| line.trim_start().starts_with("quad"))
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains(&format!("order={below}")) && lines[0].contains("1, 0, 0"));
    assert!(lines[1].contains(&format!("order={above}")) && lines[1].contains("0, 0, 1"));
}

#[test]
fn the_damage_the_frame_was_drawn_against_is_recorded() {
    let scene = kitchen_sink();
    let mut narrow = DamageSet::new();
    narrow.absorb(Rect::new(Point::new(0, 0), Size::new(16, 16)));

    let full = transcript::of(&scene, &DamageSet::full());
    let partial = transcript::of(&scene, &narrow);

    assert!(full.as_str().contains("damage full"));
    assert!(partial.as_str().contains("damage rects=1"));
    assert!(partial.as_str().contains("rect(0, 0, 16, 16)"));
}

#[test]
#[should_panic(expected = "needs a finished scene")]
fn a_transcript_of_an_unfinished_scene_is_refused() {
    // The arrays are not in draw order until the scene is finished, so a transcript taken from them
    // would be a perfectly stable rendering of a sequence no renderer will ever draw.
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(8, 8));
    let _ = transcript::of(&scene, &DamageSet::full());
}

#[test]
fn the_kitchen_sink_matches_its_golden() {
    let rendered = transcript::of(&kitchen_sink(), &DamageSet::full());
    golden::assert_matches(&golden_path("scene/kitchen_sink.txt"), rendered.as_str());
}

/// A scene with one quad of `color`.
fn one_quad(color: Color) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(64, 64));
    let fill = PaintRef::solid(scene.paints.solid(color));
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 16.0, 16.0), fill));
    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn every_field_that_is_printed_only_when_it_is_set_is_printed_when_it_is_set() {
    // The fields omitted at their default are the ones no default-shaped scene can see. Deleting
    // any of the conditional branches that print them leaves the kitchen-sink golden green and the
    // stability test green, because none of them is ever reached there — so each is exercised here
    // with its value moved off the default.
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(256, 256));
    let fill = PaintRef::solid(scene.paints.solid(Color::BLACK));

    scene.push_shadow(zgui_scene::Shadow::inset_shadow(
        rect(8.0, 8.0, 64.0, 32.0),
        (1.0, 1.0),
        0.0,
        2.0,
        Color::BLACK,
    ));

    let mut sprite = zgui_scene::ColorSprite::new(
        rect(8.0, 48.0, 16.0, 16.0),
        support::tile(zgui_atlas::TextureKind::Color, 0),
    );
    sprite.opacity = 0.25;
    sprite.flags |= zgui_scene::ColorSprite::GRAYSCALE;
    scene.push_color_sprite(sprite);

    let mut external = zgui_scene::ExternalQuad::new(
        rect(8.0, 72.0, 32.0, 32.0),
        zgui_scene::ExternalTextureId(3),
    );
    external.opacity = 0.5;
    scene.push_external(external);

    let mut path = zgui_scene::kurbo::BezPath::new();
    path.move_to((8.0, 120.0));
    path.line_to((72.0, 120.0));
    scene.push_vector(zgui_scene::VectorItem::stroked(
        zgui_scene::VectorId(1),
        std::sync::Arc::new(path),
        fill,
        3.0,
    ));

    let mut dashed = Quad::filled(rect(8.0, 140.0, 64.0, 32.0), fill).with_border(
        [1.0; 4],
        fill,
        zgui_scene::prim::BorderStyle::Dashed,
    );
    dashed.style |= 5 << 8;
    scene.push_quad(dashed);

    scene.finish(&DamageSet::full());
    let text = transcript::of(&scene, &DamageSet::full()).into_string();

    for field in [
        " inset",
        "opacity=0.25",
        "grayscale",
        "opacity=0.5",
        "stroke=solid srgb(0, 0, 0, 1) width=3",
        "style=dashed+5",
    ] {
        assert!(
            text.contains(field),
            "the transcript never prints `{field}`:\n{text}"
        );
    }
}
