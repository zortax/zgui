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

/// A union of two discs, a box with a border and a round-capped polyline, with every payload
/// position and length multiplied by `k`, mapped to local space by `axes` and `[3, 4]`.
fn push_scaled_marks(scene: &mut Scene, k: f32, axes: [f32; 4]) {
    use zgui_scene::{MarkBox, MarkFlags, MarkItem, MarkPayload};

    let paint = PaintRef::solid(scene.paints.solid(Color::srgb_u8(200, 60, 20, 255)));
    let mut push = |payload: MarkPayload, bounds: [f32; 4], flags: u32, half_width: f32| {
        let mut item = MarkItem::new(
            rect(bounds[0], bounds[1], bounds[2], bounds[3]),
            paint,
            payload.counts(),
        );
        item.flags = flags;
        item.origin = [3.0, 4.0];
        item.axes = axes;
        item.half_width = half_width * k;
        scene.push_marks(item, Arc::new(payload));
    };
    push(
        MarkPayload {
            discs: vec![
                [6.1 * k, 6.3 * k, 3.7 * k, 0.0],
                [9.0 * k, 7.0 * k, 3.1 * k, 1.2 * k],
            ],
            ..MarkPayload::default()
        },
        [0.0, 0.0, 40.0, 40.0],
        MarkFlags::UNION,
        0.0,
    );
    push(
        MarkPayload {
            boxes: vec![MarkBox {
                rect: [20.2 * k, 3.1 * k, 34.7 * k, 11.9 * k],
                radii: [2.0 * k; 8],
                shape: [2.0, 1.3 * k, 0.0, 0.0],
            }],
            ..MarkPayload::default()
        },
        [50.0, 0.0, 80.0, 40.0],
        0,
        0.0,
    );
    let nan = f32::NAN;
    push(
        MarkPayload {
            vertices: vec![
                [nan, nan],
                [4.0 * k, 20.0 * k],
                [14.3 * k, 31.1 * k],
                [30.2 * k, 22.4 * k],
                [nan, nan],
            ],
            ..MarkPayload::default()
        },
        [0.0, 40.0, 100.0, 90.0],
        MarkFlags::UNION | MarkFlags::caps(MarkFlags::ROUND, MarkFlags::ROUND),
        1.1,
    );
}

#[test]
fn mark_axes_draw_as_the_mapped_payload() {
    let draw = |k: f32, axes: [f32; 4]| {
        let mut renderer = plain_renderer()?;
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(SIDE, SIDE));
        push_scaled_marks(&mut scene, k, axes);
        scene.finish(&DamageSet::full());
        Some(draw_bytes(&mut renderer, &scene).1)
    };
    let Some(mapped) = draw(1.0, [2.5, 0.0, 0.0, 2.5]) else {
        return;
    };
    let Some(expected) = draw(2.5, [1.0, 0.0, 0.0, 1.0]) else {
        return;
    };
    assert!(expected.rgba(18, 19)[3] > 200, "the control draws");
    assert!(
        mapped.max_difference(&expected) <= 1,
        "axes map the payload as a pre-scaled payload draws: {}",
        mapped.max_difference(&expected)
    );
}

/// The payloads of [`push_marks`] at origin zero, made once and shared by every encoding.
fn shared_payloads() -> [Arc<zgui_scene::MarkPayload>; 2] {
    use zgui_scene::MarkPayload;

    [
        Arc::new(MarkPayload {
            discs: vec![
                [20.5, 20.5, 9.25, 0.0],
                [28.5, 24.5, 9.25, 0.0],
                [24.5, 30.5, 6.5, 0.0],
            ],
            ..MarkPayload::default()
        }),
        Arc::new(MarkPayload {
            discs: vec![[80.5, 40.5, 12.0, 7.5]],
            ..MarkPayload::default()
        }),
    ]
}

/// The marks of [`push_marks`] over `payloads`, with their origin at `(across, down)`.
fn push_shared_marks(
    scene: &mut Scene,
    payloads: &[Arc<zgui_scene::MarkPayload>; 2],
    across: f32,
    down: f32,
) {
    use zgui_scene::{MarkFlags, MarkItem};

    let translucent = PaintRef::solid(scene.paints.solid(Color::srgb_u8(200, 60, 20, 128)));
    let opaque = PaintRef::solid(scene.paints.solid(Color::srgb_u8(20, 160, 90, 255)));
    let mut union = MarkItem::new(
        rect(11.25 + across, 11.25 + down, 26.5, 28.5),
        translucent,
        payloads[0].counts(),
    );
    union.flags |= MarkFlags::UNION;
    union.origin = [across, down];
    scene.push_marks(union, Arc::clone(&payloads[0]));
    let mut direct = MarkItem::new(
        rect(68.5 + across, 28.5 + down, 24.0, 24.0),
        opaque,
        payloads[1].counts(),
    );
    direct.origin = [across, down];
    scene.push_marks(direct, Arc::clone(&payloads[1]));
}

/// Encodes the shared marks at `(across, down)` as chunk `revision`, retiring `retired`, and
/// draws it. Returns the payload bytes the frame uploaded and its pixels.
fn encode_shared(
    renderer: &mut support::TestRenderer,
    scene: &mut Scene,
    payloads: &[Arc<zgui_scene::MarkPayload>; 2],
    (across, down): (f32, f32),
    revision: u64,
    retired: &[u64],
) -> (u64, Pixels) {
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.begin_chunk_capture(ChunkPrims::default());
    push_shared_marks(scene, payloads, across, down);
    let chunk = Arc::new(scene.take_chunk_capture());
    scene.note_chunk_inserted(revision, chunk);
    for &old in retired {
        scene.note_chunk_retired(old);
    }
    scene.bind_capture(revision);
    scene.finish(&DamageSet::full());
    zgui_profile::counter::reset();
    let (_, pixels) = draw_bytes(renderer, scene);
    scene.clear_chunk_notes();
    (
        zgui_profile::counter::get(zgui_profile::Counter::MarksPayloadBytes),
        pixels,
    )
}

#[test]
fn a_re_encoded_chunk_sharing_its_payload_uploads_none() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let payloads = shared_payloads();
    let mut scene = Scene::new();
    let (first, _) = encode_shared(&mut renderer, &mut scene, &payloads, (0.0, 0.0), 1, &[]);
    let (second, panned) = encode_shared(&mut renderer, &mut scene, &payloads, (5.0, 7.0), 2, &[1]);
    if zgui_profile::COUNTERS_ENABLED {
        assert!(first > 0, "the first encoding uploads its payload");
        assert_eq!(second, 0, "the next revision holds the same payload");
    }

    drop(renderer);
    let Some(mut control_renderer) = plain_renderer() else {
        return;
    };
    let mut control = Scene::new();
    control.begin_frame(Size::new(SIDE, SIDE));
    push_marks_at(&mut control, 5.0, 7.0);
    control.finish(&DamageSet::full());
    let (_, expected) = draw_bytes(&mut control_renderer, &control);
    assert_eq!(panned.max_difference(&expected), 0);
}

#[test]
fn a_shared_payload_outlives_its_first_holder() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let payloads = shared_payloads();
    let mut scene = Scene::new();
    // Two chunks hold the payload: the first is encoded, then the second.
    encode_shared(&mut renderer, &mut scene, &payloads, (0.0, 0.0), 1, &[]);
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.begin_chunk_capture(ChunkPrims::default());
    push_shared_marks(&mut scene, &payloads, 5.0, 7.0);
    let second = Arc::new(scene.take_chunk_capture());
    scene.note_chunk_inserted(2, Arc::clone(&second));
    scene.bind_capture(2);
    scene.finish(&DamageSet::full());
    draw_bytes(&mut renderer, &scene);
    scene.clear_chunk_notes();
    // The first retires, and the second replays in place.
    let mut last = None;
    for frame in 0..4 {
        scene.begin_frame(Size::new(SIDE, SIDE));
        scene.replay_chunk(&second, Size::new(DevicePx(0.0), DevicePx(0.0)), 2);
        if frame == 0 {
            scene.note_chunk_retired(1);
        }
        scene.finish(&DamageSet::full());
        zgui_profile::counter::reset();
        let (_, pixels) = draw_bytes(&mut renderer, &scene);
        scene.clear_chunk_notes();
        if zgui_profile::COUNTERS_ENABLED {
            assert_eq!(
                zgui_profile::counter::get(zgui_profile::Counter::MarksPayloadBytes),
                0,
                "frame {frame}: the payload stays resident under its second holder"
            );
        }
        last = Some(pixels);
    }
    // A third chunk over the same payload after many frames finds it still resident.
    let (third, _) = encode_shared(&mut renderer, &mut scene, &payloads, (5.0, 7.0), 3, &[2]);
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(
            third, 0,
            "the payload passed from the second holder to the third"
        );
    }

    drop(renderer);
    let Some(mut control_renderer) = plain_renderer() else {
        return;
    };
    let mut control = Scene::new();
    control.begin_frame(Size::new(SIDE, SIDE));
    push_marks_at(&mut control, 5.0, 7.0);
    control.finish(&DamageSet::full());
    let (_, expected) = draw_bytes(&mut control_renderer, &control);
    assert_eq!(last.expect("drawn").max_difference(&expected), 0);
}

#[test]
fn shared_payloads_hold_still_over_a_hundred_pans() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let payloads = shared_payloads();
    let mut scene = Scene::new();
    let mut held = None;
    for frame in 1..=100_u64 {
        let retired: &[u64] = if frame > 1 { &[frame - 1] } else { &[] };
        let at = ((frame % 9) as f32, (frame % 5) as f32);
        let (bytes, _) = encode_shared(&mut renderer, &mut scene, &payloads, at, frame, retired);
        if zgui_profile::COUNTERS_ENABLED && frame > 1 {
            assert_eq!(bytes, 0, "frame {frame} uploads no payload");
        }
        if frame == 10 {
            held = Some(renderer.memory().buffers);
        }
    }
    assert_eq!(Some(renderer.memory().buffers), held);
}

/// Uploads a sheet of sixteen one-texel cells, cell `p` holding a level of its own, and returns
/// a glyph payload with one copy per phase, and the packed texture.
fn glyph_sheet(
    renderer: &mut zgui_render_wgpu::WgpuRenderer,
) -> (Arc<zgui_scene::MarkPayload>, u32) {
    use zgui_atlas::{Atlas, AtlasKey, AtlasLimits, TextureKind};

    let mut atlas = Atlas::new(AtlasLimits::default());
    let tile = atlas
        .get_or_insert(
            AtlasKey::new(0x7E00_0000_0000_0002, TextureKind::Mono),
            Size::new(4, 4),
            || {
                (0..16)
                    .map(|phase| 15 * (phase + 1) as u8)
                    .collect::<Vec<u8>>()
            },
        )
        .expect("a fresh atlas has room");
    atlas
        .flush_uploads(renderer.atlas())
        .expect("the device accepts the upload");
    let (x, y) = (tile.bounds.origin.x as u32, tile.bounds.origin.y as u32);
    let mut glyphs: Vec<[u32; 4]> = (0..16)
        .map(|phase| {
            [
                (x + phase % 4) | ((y + phase / 4) << 16),
                1 | (1 << 16),
                0,
                0,
            ]
        })
        .collect();
    for phase in 0..16u32 {
        let (px, py) = ((phase % 4) as f32, (phase / 4) as f32);
        let anchor = [
            10.0 + 20.0 * px + px / 4.0 + 0.01,
            10.0 + 20.0 * py + py / 4.0 + 0.01,
        ];
        let offset = glyphs.len() as u32;
        glyphs.push([anchor[0].to_bits(), anchor[1].to_bits(), 0, offset]);
    }
    let payload = zgui_scene::MarkPayload {
        glyphs,
        ..zgui_scene::MarkPayload::default()
    };
    (Arc::new(payload), zgui_scene::SpriteTile::of(tile).texture)
}

/// Pushes one union glyph mark over `payload`, moved by `by`.
fn push_glyphs(
    scene: &mut Scene,
    payload: &Arc<zgui_scene::MarkPayload>,
    texture: u32,
    by: [f32; 2],
) {
    use zgui_scene::{MarkFlags, MarkItem};

    let white = PaintRef::solid(scene.paints.solid(Color::srgb_u8(255, 255, 255, 255)));
    let mut item = MarkItem::new(rect(8.0, 8.0, 80.0, 80.0), white, payload.counts());
    item.tiles = 16;
    item.texture = texture;
    item.flags |= MarkFlags::UNION;
    item.reanchor(Size::new(DevicePx(by[0]), DevicePx(by[1])));
    scene.push_marks(item, Arc::clone(payload));
}

#[test]
fn a_moved_glyph_chunk_uploads_no_payload() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let (payload, texture) = glyph_sheet(&mut renderer);

    // Frame one: the chunk is captured and made resident.
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.begin_chunk_capture(ChunkPrims::default());
    push_glyphs(&mut scene, &payload, texture, [0.0, 0.0]);
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
    assert_eq!(first.rgba(10, 10)[3], 15, "phase zero reads its own cell");
    scene.clear_chunk_notes();

    // Frame two: the same chunk replayed a quarter pixel past five across and half past seven
    // down, which moves every copy to another phase.
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.replay_chunk(&chunk, Size::new(DevicePx(5.25), DevicePx(7.5)), 1);
    scene.finish(&DamageSet::full());
    zgui_profile::counter::reset();
    let (_, moved) = draw_bytes(&mut renderer, &scene);
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(
            zgui_profile::counter::get(zgui_profile::Counter::MarksPayloadBytes),
            0,
            "a moved replay of a resident glyph chunk uploads no payload"
        );
    }
    // Phase (0, 0) moved by (1, 2) quarters lands on pixel (15, 17) and reads cell 9.
    assert_eq!(
        moved.rgba(15, 17)[3],
        150,
        "the moved copy reads its new phase"
    );

    // The control: the same glyphs encoded fresh at the moved position, on a renderer of its own.
    drop(renderer);
    let Some(mut control_renderer) = plain_renderer() else {
        return;
    };
    let (payload, texture) = glyph_sheet(&mut control_renderer);
    let mut control = Scene::new();
    control.begin_frame(Size::new(SIDE, SIDE));
    push_glyphs(&mut control, &payload, texture, [5.25, 7.5]);
    control.finish(&DamageSet::full());
    let (_, expected) = draw_bytes(&mut control_renderer, &control);
    assert_eq!(
        moved.max_difference(&expected),
        0,
        "the offset draw finds every phase a fresh encoding finds"
    );
}
