//! The startup pattern: a known scene composed, copied and read back on a real device.
//!
//! It is the test that fails loudly when a driver does something unexpected with the path a frame
//! actually takes — the storage buffers the instances live in, the signed-distance coverage, the
//! blend, the copy to the surface. Everything else here assumes that path works; this is what
//! checks it.

mod support;

use zgui_bits::DamageSet;
use zgui_render::Renderer;
use zgui_scene::prim::BorderStyle;
use zgui_scene::{Quad, Scene};

use support::{SIDE, opaque, plain_renderer, present, rect};

/// The pattern: a red field, a green square over its middle, and a blue border round the square.
fn pattern(scene: &mut Scene) {
    let field = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(255, 0, 0)));
    let square = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(0, 255, 0)));
    let border = scene
        .paints
        .add(zgui_scene::Paint::Solid(opaque(0, 0, 255)));

    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        field,
    ));
    scene.push_quad(
        Quad::filled(rect(32.0, 32.0, 64.0, 64.0), square).with_border(
            [8.0; 4],
            border,
            BorderStyle::Solid,
        ),
    );
}

#[test]
fn a_known_pattern_survives_composition_and_the_copy_to_the_surface() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(zgui_geom::Size::new(SIDE, SIDE));
    pattern(&mut scene);
    scene.finish(&DamageSet::full());

    let pixels = present(&mut renderer, &scene);

    assert_eq!(pixels.rgba(4, 4), [255, 0, 0, 255], "the field");
    assert_eq!(pixels.rgba(64, 64), [0, 255, 0, 255], "the square");
    assert_eq!(pixels.rgba(64, 36), [0, 0, 255, 255], "the border");
    assert_eq!(
        pixels.rgba(64, 28),
        [255, 0, 0, 255],
        "just outside the square"
    );

    // The whole rectangle, read the way a caller copying it out reads it. A scanout takes the
    // bytes in one go and is told which order they are in, so the two ways of reading one pixel
    // have to agree at every pixel.
    let size = pixels.size();
    let bytes = pixels.bytes();
    assert_eq!(
        bytes.len(),
        (size.width * size.height * 4) as usize,
        "the bytes are tightly packed, four to a pixel"
    );
    for y in 0..size.height {
        for x in 0..size.width {
            let offset = ((y * size.width + x) * 4) as usize;
            let raw: [u8; 4] = bytes[offset..offset + 4]
                .try_into()
                .expect("four bytes make a pixel");
            let expected = if pixels.is_bgra() {
                [raw[2], raw[1], raw[0], raw[3]]
            } else {
                raw
            };
            assert_eq!(
                pixels.rgba(x, y),
                expected,
                "the bytes at ({x}, {y}) are the pixel read there"
            );
        }
    }
}

#[test]
fn what_was_composed_is_what_was_presented() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(zgui_geom::Size::new(SIDE, SIDE));
    pattern(&mut scene);
    scene.finish(&DamageSet::full());

    let presented = present(&mut renderer, &scene);
    let composed = renderer.read_composed();
    assert_eq!(
        composed.max_difference(&presented),
        0,
        "the copy to the surface changes no value"
    );
}

#[test]
fn a_frame_reports_what_it_drew() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(zgui_geom::Size::new(SIDE, SIDE));
    pattern(&mut scene);
    scene.finish(&DamageSet::full());

    let stats = renderer
        .draw(&scene, &DamageSet::full())
        .stats()
        .expect("the frame reached its target");
    assert_eq!(
        stats.draw_calls, 3,
        "clearing the damaged rectangle, two quads at different orders, and the copy"
    );
    assert_eq!(
        stats.damage_px,
        (SIDE * SIDE) as u64,
        "the whole surface was redrawn"
    );
    assert!(
        stats.memory.targets > 0,
        "the composed target is held between frames"
    );
    assert!(stats.bytes_uploaded > 0, "the instances reached the device");
}

#[test]
fn a_second_frame_of_the_same_scene_is_identical_to_the_first() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(zgui_geom::Size::new(SIDE, SIDE));
    pattern(&mut scene);
    scene.finish(&DamageSet::full());

    let first = present(&mut renderer, &scene);
    let second = present(&mut renderer, &scene);
    assert_eq!(
        first.max_difference(&second),
        0,
        "composing the same scene twice must produce the same pixels"
    );
}

#[test]
fn an_unchanged_second_frame_does_not_upload_side_tables_again() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(zgui_geom::Size::new(SIDE, SIDE));
    pattern(&mut scene);
    scene.finish(&DamageSet::full());

    let first = renderer
        .draw(&scene, &DamageSet::full())
        .stats()
        .expect("the first frame reached its target")
        .bytes_uploaded;
    let second = renderer
        .draw(&scene, &DamageSet::full())
        .stats()
        .expect("the second frame reached its target")
        .bytes_uploaded;

    assert!(
        second < first,
        "the unchanged frame uploads instances and blocks ({second} bytes), not the side tables from the first frame ({first} bytes)"
    );
}
