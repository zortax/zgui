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

/// A chunk holding one union mark of overlapping discs and one direct mark of a ring, `down` from
/// the top, with its payload coordinates measured from `origin`.
fn push_marks(scene: &mut Scene, down: f32, origin: [f32; 2]) {
    use zgui_scene::{MarkFlags, MarkItem, MarkPayload};

    let translucent = PaintRef::solid(scene.paints.solid(Color::srgb_u8(200, 60, 20, 128)));
    let opaque = PaintRef::solid(scene.paints.solid(Color::srgb_u8(20, 160, 90, 255)));
    let at = |x: f32, y: f32| [x - origin[0], y + down - origin[1]];
    let overlapping = MarkPayload {
        discs: vec![
            [at(20.5, 20.5)[0], at(20.5, 20.5)[1], 9.25, 0.0],
            [at(28.5, 24.5)[0], at(28.5, 24.5)[1], 9.25, 0.0],
            [at(24.5, 30.5)[0], at(24.5, 30.5)[1], 6.5, 0.0],
        ],
        ..MarkPayload::default()
    };
    let mut union = MarkItem::new(
        rect(11.25, 11.25 + down, 26.5, 28.5),
        translucent,
        overlapping.counts(),
    );
    union.flags |= MarkFlags::UNION;
    union.origin = origin;
    scene.push_marks(union, Arc::new(overlapping));
    let ring = MarkPayload {
        discs: vec![[at(80.5, 40.5)[0], at(80.5, 40.5)[1], 12.0, 7.5]],
        ..MarkPayload::default()
    };
    let mut direct = MarkItem::new(rect(68.5, 28.5 + down, 24.0, 24.0), opaque, ring.counts());
    direct.origin = origin;
    scene.push_marks(direct, Arc::new(ring));
}

#[test]
fn a_moved_marks_chunk_uploads_no_payload() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };

    // Frame one: the chunk is captured and made resident.
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.begin_chunk_capture(ChunkPrims::default());
    push_marks(&mut scene, 0.0, [0.0, 0.0]);
    let chunk = Arc::new(scene.take_chunk_capture());
    scene.note_chunk_inserted(1, Arc::clone(&chunk));
    scene.bind_capture(1);
    scene.finish(&DamageSet::full());
    zgui_profile::counter::reset();
    let (_, first) = draw_bytes(&mut renderer, &scene);
    if zgui_profile::COUNTERS_ENABLED {
        assert!(
            zgui_profile::counter::get(zgui_profile::Counter::MarksPayloadBytes) > 0,
            "the encoded chunk uploads its payload once"
        );
    }
    let overlap = first.rgba(24, 24)[3];
    assert!(
        (127..=129).contains(&overlap),
        "three overlapping translucent discs paint their alpha once: {overlap}"
    );
    assert_eq!(first.rgba(80, 30)[3], 255, "the ring draws");
    assert_eq!(first.rgba(80, 40)[3], 0, "and leaves its hole");
    scene.clear_chunk_notes();

    // Frame two: the same chunk replayed five pixels across and seven down.
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.replay_chunk(&chunk, Size::new(DevicePx(5.0), DevicePx(7.0)), 1);
    scene.finish(&DamageSet::full());
    zgui_profile::counter::reset();
    let (_, moved) = draw_bytes(&mut renderer, &scene);
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(
            zgui_profile::counter::get(zgui_profile::Counter::MarksPayloadBytes),
            0,
            "a moved replay of a resident chunk uploads no payload"
        );
    }

    // The control: the same marks encoded fresh at the moved position, on a renderer of its own.
    drop(renderer);
    let Some(mut control_renderer) = plain_renderer() else {
        return;
    };
    let mut control = Scene::new();
    control.begin_frame(Size::new(SIDE, SIDE));
    push_marks_at(&mut control, 5.0, 7.0);
    control.finish(&DamageSet::full());
    let (_, expected) = draw_bytes(&mut control_renderer, &control);
    assert_eq!(
        moved.max_difference(&expected),
        0,
        "the offset draw puts every prim exactly where a fresh encoding would"
    );
}

#[test]
fn a_moved_marks_chunk_with_an_origin_draws_as_a_fresh_encoding() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let counted = |what: &str, uploads: bool| {
        if zgui_profile::COUNTERS_ENABLED {
            let bytes = zgui_profile::counter::get(zgui_profile::Counter::MarksPayloadBytes);
            assert_eq!(bytes > 0, uploads, "{what} uploads {bytes} payload bytes");
        }
    };

    // Frame one: the payload is measured from an origin away from zero, and the chunk is made
    // resident.
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.begin_chunk_capture(ChunkPrims::default());
    push_marks(&mut scene, 0.0, [12.0, -6.0]);
    let chunk = Arc::new(scene.take_chunk_capture());
    scene.note_chunk_inserted(1, Arc::clone(&chunk));
    scene.bind_capture(1);
    scene.finish(&DamageSet::full());
    zgui_profile::counter::reset();
    draw_bytes(&mut renderer, &scene);
    counted("the encoded chunk", true);
    scene.clear_chunk_notes();

    // Frame two: a moved replay of the resident chunk, drawn through an offset slot.
    let moved = |scene: &mut Scene, renderer: &mut support::TestRenderer| {
        scene.begin_frame(Size::new(SIDE, SIDE));
        scene.replay_chunk(&chunk, Size::new(DevicePx(5.0), DevicePx(7.0)), 1);
        scene.finish(&DamageSet::full());
        zgui_profile::counter::reset();
        draw_bytes(renderer, scene).1
    };
    let resident = moved(&mut scene, &mut renderer);
    counted("a moved replay of a resident chunk", false);

    // Frame three: the same replay after the residence is released, gathered from the frame
    // arrays.
    renderer.release_idle_resources();
    let transient = moved(&mut scene, &mut renderer);
    counted("a replay of a released chunk", true);

    // The control: the same marks encoded fresh at the moved position, on a renderer of its own.
    drop(renderer);
    let Some(mut control_renderer) = plain_renderer() else {
        return;
    };
    let mut control = Scene::new();
    control.begin_frame(Size::new(SIDE, SIDE));
    push_marks_at(&mut control, 5.0, 7.0);
    control.finish(&DamageSet::full());
    let (_, expected) = draw_bytes(&mut control_renderer, &control);
    assert_eq!(
        resident.max_difference(&expected),
        0,
        "the offset slot adds the origin as a fresh encoding does"
    );
    assert_eq!(
        transient.max_difference(&expected),
        0,
        "the transient gather adds the moved origin as a fresh encoding does"
    );
}

/// The marks of [`push_marks`], encoded at `(across, down)` with their payload there too.
fn push_marks_at(scene: &mut Scene, across: f32, down: f32) {
    use zgui_scene::{MarkFlags, MarkItem, MarkPayload};

    let translucent = PaintRef::solid(scene.paints.solid(Color::srgb_u8(200, 60, 20, 128)));
    let opaque = PaintRef::solid(scene.paints.solid(Color::srgb_u8(20, 160, 90, 255)));
    let overlapping = MarkPayload {
        discs: vec![
            [20.5 + across, 20.5 + down, 9.25, 0.0],
            [28.5 + across, 24.5 + down, 9.25, 0.0],
            [24.5 + across, 30.5 + down, 6.5, 0.0],
        ],
        ..MarkPayload::default()
    };
    let mut union = MarkItem::new(
        rect(11.25 + across, 11.25 + down, 26.5, 28.5),
        translucent,
        overlapping.counts(),
    );
    union.flags |= MarkFlags::UNION;
    scene.push_marks(union, Arc::new(overlapping));
    let ring = MarkPayload {
        discs: vec![[80.5 + across, 40.5 + down, 12.0, 7.5]],
        ..MarkPayload::default()
    };
    let direct = MarkItem::new(
        rect(68.5 + across, 28.5 + down, 24.0, 24.0),
        opaque,
        ring.counts(),
    );
    scene.push_marks(direct, Arc::new(ring));
}

#[test]
fn marks_arenas_hold_still_over_a_hundred_redraws() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    let mut held = None;
    for frame in 1..=100_u64 {
        // One fresh encoding per frame, and the one before it retired.
        scene.begin_frame(Size::new(SIDE, SIDE));
        scene.begin_chunk_capture(ChunkPrims::default());
        push_marks(&mut scene, (frame % 7) as f32, [0.0, 0.0]);
        let chunk = Arc::new(scene.take_chunk_capture());
        scene.note_chunk_inserted(frame, chunk);
        if frame > 1 {
            scene.note_chunk_retired(frame - 1);
        }
        scene.bind_capture(frame);
        scene.finish(&DamageSet::full());
        draw_bytes(&mut renderer, &scene);
        scene.clear_chunk_notes();
        if frame == 10 {
            held = Some(renderer.memory().buffers);
        }
    }
    assert_eq!(
        Some(renderer.memory().buffers),
        held,
        "retired payloads are reused, so the arenas do not grow"
    );
}
