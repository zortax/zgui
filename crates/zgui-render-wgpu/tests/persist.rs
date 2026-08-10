//! A chunk resident in the arenas is drawn without its bytes being uploaded again.
//!
//! The paint cache notes an encoded chunk into the scene; the renderer uploads it once and keeps
//! it resident. A later frame that replays the chunk in place stamps its primitives' provenance,
//! and the resolved remap points the draw at the resident bytes — so what that frame uploads is
//! the remap and the frame's own small tables, and none of the primitives. A replay that moved is
//! transient again: its bytes differ from the resident copy by the translation.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{DevicePx, Size};
use zgui_render_wgpu::Pixels;
use zgui_scene::{ChunkPrims, PaintRef, Quad, Scene};

use zgui_render::Renderer;

use support::{SIDE, plain_renderer, rect};

/// How many quads the chunk holds — enough that the primitive bytes dominate the frame's fixed
/// upload costs, so the comparison discriminates.
const QUADS: usize = 100;

/// Pushes the chunk's quads: a column of small distinct rectangles.
fn push_quads(scene: &mut Scene, fill: PaintRef) {
    for at in 0..QUADS {
        let y = 2.0 + (at as f32) * 2.0;
        scene.push_quad(Quad::filled(rect(8.0, y, 64.0, 1.5), fill));
    }
}

/// One frame's uploaded bytes and pixels, drawn over full damage.
fn draw_bytes(renderer: &mut support::TestRenderer, scene: &Scene) -> (u64, Pixels) {
    let outcome = renderer.draw(scene, &DamageSet::full());
    let bytes = outcome
        .stats()
        .expect("a frame composed into a texture always reaches it")
        .bytes_uploaded;
    let pixels = renderer
        .read_presented()
        .expect("a stand-in surface can be read back");
    (bytes, pixels)
}

#[test]
fn a_resident_chunk_replayed_in_place_uploads_no_primitive_bytes() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };

    // Frame one: the chunk is captured, noted, and drawn — the renderer makes it resident.
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let fill = PaintRef::solid(scene.paints.solid(Color::srgb_u8(40, 120, 200, 255)));
    scene.begin_chunk_capture(ChunkPrims::default());
    push_quads(&mut scene, fill);
    let chunk = Arc::new(scene.take_chunk_capture());
    scene.note_chunk_inserted(1, Arc::clone(&chunk));
    scene.bind_capture(1);
    scene.finish(&DamageSet::full());
    let (first_bytes, first) = draw_bytes(&mut renderer, &scene);
    scene.clear_chunk_notes();

    // Frame two: the same painting, replayed in place out of the chunk.
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.replay_chunk(&chunk, Size::default(), 1);
    scene.finish(&DamageSet::full());
    let (second_bytes, second) = draw_bytes(&mut renderer, &scene);

    assert_eq!(
        second.max_difference(&first),
        0,
        "the resident bytes draw exactly what the uploaded frame drew"
    );
    assert!(
        second_bytes * 4 < first_bytes,
        "a frame replaying a resident chunk uploads the remap and its tables, never the \
         primitives: {second_bytes} against {first_bytes}"
    );

    // Frame three: the same chunk, replayed eight pixels down. The bytes differ from the
    // resident copy by the translation, so the frame is transient again and pays its uploads.
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(8.0)), 1);
    scene.finish(&DamageSet::full());
    let (moved_bytes, _) = draw_bytes(&mut renderer, &scene);
    assert!(
        moved_bytes > second_bytes * 2,
        "a moved replay's bytes are its own: {moved_bytes} against {second_bytes}"
    );
}
