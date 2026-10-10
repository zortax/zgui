//! A chunk resident in the arenas is drawn without its bytes being uploaded again.
//!
//! The paint cache notes an encoded chunk into the scene; the renderer uploads it once and keeps
//! it resident. A later frame that replays the chunk in place stamps its primitives' provenance,
//! and the resolved remap points the draw at the resident bytes — so what that frame uploads is
//! the remap and the frame's own small tables, and none of the primitives. A replay that moved
//! keeps its residence too: the chunk's offset rides the remap's high bits and the shader adds it
//! at draw time, so a drag re-uploads nothing but the remap and one small offset table.

mod support;

use std::sync::{Arc, OnceLock};

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{DevicePx, Size};
use zgui_render_wgpu::{EffectProgram, ParamsField, ParamsLayout, Pixels};
use zgui_scene::{ChunkPrims, PaintRef, Quad, Scene, ShadedQuad, ShaderId, ShaderParams};
use zgui_wgsl::ShaderMode;

use zgui_render::Renderer;

use support::{SIDE, plain_renderer, rect};

/// How many quads the chunk holds — enough that the primitive bytes dominate the frame's fixed
/// upload costs, so the comparison discriminates.
const QUADS: usize = 100;

/// Pushes the chunk's quads: a column of small distinct rectangles, `down` from the top.
fn push_quads_at(scene: &mut Scene, fill: PaintRef, down: f32) {
    for at in 0..QUADS {
        let y = down + 2.0 + (at as f32) * 2.0;
        scene.push_quad(Quad::filled(rect(8.0, y, 64.0, 1.5), fill));
    }
}

/// Pushes the chunk's quads where the capture takes them.
fn push_quads(scene: &mut Scene, fill: PaintRef) {
    push_quads_at(scene, fill, 0.0);
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
        "a frame replaying a resident chunk uploads at most the remap and its tables, never the \
         primitives: {second_bytes} against {first_bytes}"
    );
    assert!(
        second_bytes < (QUADS * 4) as u64,
        "the resolved remap matched the first frame's — insertion and provenance land in one \
         pass — so even the remap upload is skipped: {second_bytes}"
    );

    // Frame two again: the same painting once more. The resolved remap is the list the buffer
    // already holds, so even the remap upload is skipped and the frame's bytes fall further.
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.replay_chunk(&chunk, Size::default(), 1);
    scene.finish(&DamageSet::full());
    let (steady_bytes, steady) = draw_bytes(&mut renderer, &scene);
    assert_eq!(steady.max_difference(&first), 0);
    assert!(
        steady_bytes <= second_bytes,
        "a steady frame never owes more than the one before it: {steady_bytes} against \
         {second_bytes}"
    );

    // Frame three: the same chunk, replayed eight pixels down. The resident bytes stay where
    // they are; the offset rides the remap's high bits and the shader adds it, so the frame
    // uploads the remap, one small offset table, and none of the primitives.
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.replay_chunk(&chunk, Size::new(DevicePx(0.0), DevicePx(8.0)), 1);
    scene.finish(&DamageSet::full());
    let (moved_bytes, moved) = draw_bytes(&mut renderer, &scene);
    assert!(
        moved_bytes * 4 < first_bytes,
        "a moved replay re-uploads its primitives: {moved_bytes} against {first_bytes}"
    );

    // The control: the same quads encoded fresh at the moved position, through a renderer of its
    // own so the two draws share nothing. Sequential, because one machine offers one device: the
    // resident draw's renderer is over before the control's is made.
    drop(renderer);
    let Some(mut control_renderer) = plain_renderer() else {
        return;
    };
    let mut control = Scene::new();
    control.begin_frame(Size::new(SIDE, SIDE));
    let fill = PaintRef::solid(control.paints.solid(Color::srgb_u8(40, 120, 200, 255)));
    push_quads_at(&mut control, fill, 8.0);
    control.finish(&DamageSet::full());
    let (_, expected) = draw_bytes(&mut control_renderer, &control);
    assert_eq!(
        moved.max_difference(&expected),
        0,
        "the offset draw puts every primitive exactly where a fresh encoding would"
    );
}

/// The assembled translation unit of the paint effect, built once.
static PAINT_UNIT: OnceLock<String> = OnceLock::new();

/// An effect returning the colour its parameters name.
const PAINT_SOURCE: &str = r#"
struct Params {
    color: vec4<f32>,
}

fn shade(in: ShaderInput, params: Params) -> vec4<f32> {
    return params.color;
}
"#;

/// Declares the paint effect.
fn paint_effect() -> ShaderId {
    let mode = ShaderMode::Paint;
    let source = PAINT_UNIT.get_or_init(|| zgui_wgsl::effect(mode, PAINT_SOURCE));
    let id = zgui_scene::declare_shader(
        "test-persist-paint",
        mode,
        zgui_scene::ShaderReads::NOTHING,
        &[],
        0.0,
    );
    zgui_render_wgpu::declare(
        id,
        EffectProgram {
            mode,
            label: "test.persist.effect",
            representation: &[],
            source,
            params: ParamsLayout {
                size: 16,
                fields: &[ParamsField {
                    name: "color",
                    offset: 0,
                    size: 16,
                }],
            },
        },
    );
    id
}

/// Pushes `count` shaded squares in one colour, in a row `down` from the top.
fn push_shaded(scene: &mut Scene, effect: ShaderId, color: [f32; 4], down: f32, count: usize) {
    let bytes: Vec<u8> = color.iter().flat_map(|c| c.to_ne_bytes()).collect();
    let params = scene.shader_params.intern(ShaderParams::of(&bytes));
    for at in 0..count {
        let x = 2.0 + (at % 16) as f32 * 7.0;
        let y = down + (at / 16) as f32 * 7.0;
        scene.push_shaded(ShadedQuad::new(rect(x, y, 5.0, 5.0), effect, params));
    }
}

#[test]
fn a_growing_shaded_lane_keeps_its_own_bytes() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let effect = paint_effect();
    let red = [1.0, 0.0, 0.0, 1.0];
    let green = [0.0, 1.0, 0.0, 1.0];

    // Frame one: a chunk of two shaded squares is made resident in the smallest shaded arena.
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.begin_chunk_capture(ChunkPrims::default());
    push_shaded(&mut scene, effect, red, 2.0, 2);
    let first = Arc::new(scene.take_chunk_capture());
    scene.note_chunk_inserted(1, Arc::clone(&first));
    scene.bind_capture(1);
    scene.finish(&DamageSet::full());
    draw_bytes(&mut renderer, &scene);
    scene.clear_chunk_notes();

    // Frame two: a second chunk outgrows the arena, and the first replays in place out of the
    // grown buffer.
    scene.begin_frame(Size::new(SIDE, SIDE));
    let first_range = scene.replay_chunk(&first, Size::default(), 1);
    assert_eq!(first_range.len(), 2);
    scene.begin_chunk_capture(ChunkPrims::default());
    push_shaded(&mut scene, effect, green, 40.0, 64);
    let second = Arc::new(scene.take_chunk_capture());
    scene.note_chunk_inserted(2, Arc::clone(&second));
    scene.bind_capture(2);
    scene.finish(&DamageSet::full());
    let (_, grown) = draw_bytes(&mut renderer, &scene);

    let [r, g, _, a] = grown.rgba(4, 4);
    assert!(
        r > 200 && g < 40 && a > 200,
        "the resident chunk draws its own squares after the lane grew: {:?}",
        grown.rgba(4, 4)
    );
    assert!(grown.rgba(4, 44)[1] > 200, "the new chunk draws as well");
}
