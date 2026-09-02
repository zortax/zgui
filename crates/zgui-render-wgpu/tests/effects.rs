//! What an application's own shader draws, on a real device.
//!
//! The whole point of the seam is that an effect goes through what a background goes through: the
//! same arena, the same draw-order permutation, the same clip function. Each test here is one half
//! of that — the effect draws what it says, and the framework applies what it always applies.

mod support;

use std::sync::OnceLock;

use zgui_bits::DamageSet;
use zgui_geom::{Corners, DevicePx, Size, Vec2};
use zgui_render_wgpu::{EffectProgram, ParamsField, ParamsLayout};
use zgui_scene::{ClipLink, Scene, ShadedQuad, ShaderId, ShaderParams};
use zgui_wgsl::ShaderMode;

use support::{SIDE, plain_renderer, present, rect};

/// The assembled translation unit of each effect, built once.
static PAINT_UNIT: OnceLock<String> = OnceLock::new();
static COVERAGE_UNIT: OnceLock<String> = OnceLock::new();

/// An effect returning a colour its parameters name.
const PAINT_SOURCE: &str = r#"
struct Params {
    color: vec4<f32>,
}

fn shade(in: ShaderInput, params: Params) -> vec4<f32> {
    return params.color;
}
"#;

/// An effect covering the left half of whatever box it is given.
const COVERAGE_SOURCE: &str = r#"
struct Params {
    split: f32,
}

fn coverage(in: ShaderInput, params: Params) -> f32 {
    return select(0.0, 1.0, in.uv.x < params.split);
}
"#;

/// The layout of a four-float parameter block.
const COLOR_LAYOUT: ParamsLayout = ParamsLayout {
    size: 16,
    fields: &[ParamsField {
        name: "color",
        offset: 0,
        size: 16,
    }],
};

/// The layout of a one-float parameter block.
const SPLIT_LAYOUT: ParamsLayout = ParamsLayout {
    size: 4,
    fields: &[ParamsField {
        name: "split",
        offset: 0,
        size: 4,
    }],
};

/// The layout of the filter effect's one-float parameter block.
const SHIFT_LAYOUT: ParamsLayout = ParamsLayout {
    size: 4,
    fields: &[ParamsField {
        name: "shift",
        offset: 0,
        size: 4,
    }],
};

/// Declares one effect, assembling its translation unit the way the macro would.
///
/// The representation is left empty on purpose, so every test here goes down the text-compiling
/// fallback — which is the path a version skew between an application's shader front end and this
/// crate's would leave behind, and the one least likely to be exercised otherwise.
fn declare(
    name: &'static str,
    mode: ShaderMode,
    snippet: &str,
    params: ParamsLayout,
    unit: &'static OnceLock<String>,
) -> ShaderId {
    let source = unit.get_or_init(|| zgui_wgsl::effect(mode, snippet));
    let id = zgui_scene::declare_shader(name, mode, zgui_scene::ShaderReads::NOTHING, &[], 0.0);
    zgui_render_wgpu::declare(
        id,
        EffectProgram {
            mode,
            label: "test.effect",
            representation: &[],
            source,
            params,
        },
    );
    id
}

/// A scene the size of the test surface.
fn scene() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene
}

/// Uniform corner radii.
fn radii(radius: f32) -> Corners<Vec2<DevicePx>> {
    let corner = Vec2::new(DevicePx(radius), DevicePx(radius));
    Corners {
        top_left: corner,
        top_right: corner,
        bottom_right: corner,
        bottom_left: corner,
    }
}

/// Four premultiplied floats as the bytes a parameter block holds.
fn color_bytes(color: [f32; 4]) -> Vec<u8> {
    color.iter().flat_map(|c| c.to_ne_bytes()).collect()
}

#[test]
fn a_paint_effect_draws_the_colour_it_returns() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-paint",
        ShaderMode::Paint,
        PAINT_SOURCE,
        COLOR_LAYOUT,
        &PAINT_UNIT,
    );

    let mut scene = scene();
    let params = scene
        .shader_params
        .intern(ShaderParams::of(&color_bytes([0.0, 0.5, 0.0, 0.5])));
    scene.push_shaded(ShadedQuad::new(rect(10.0, 10.0, 40.0, 40.0), id, params));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    let inside = pixels.rgba(30, 30);
    assert!(
        (126..=129).contains(&inside[3]),
        "the effect's own alpha reaches the surface: {inside:?}"
    );
    assert!(
        inside[1] > 100 && inside[0] < 20,
        "the effect's own colour reaches the surface: {inside:?}"
    );
    assert_eq!(pixels.rgba(5, 5)[3], 0, "nothing is drawn outside the box");
}

#[test]
fn a_paint_effect_is_bounded_by_its_own_rounded_rectangle() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-paint",
        ShaderMode::Paint,
        PAINT_SOURCE,
        COLOR_LAYOUT,
        &PAINT_UNIT,
    );

    let mut scene = scene();
    let params = scene
        .shader_params
        .intern(ShaderParams::of(&color_bytes([1.0, 1.0, 1.0, 1.0])));
    scene.push_shaded(
        ShadedQuad::new(rect(0.0, 0.0, 60.0, 60.0), id, params).with_radii(radii(20.0)),
    );
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    assert_eq!(pixels.rgba(30, 30)[3], 255, "the middle is filled");
    assert_eq!(
        pixels.rgba(1, 1)[3],
        0,
        "an effect does not escape its own corner"
    );
}

#[test]
fn an_effect_is_clipped_by_the_chain_it_draws_through() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-paint",
        ShaderMode::Paint,
        PAINT_SOURCE,
        COLOR_LAYOUT,
        &PAINT_UNIT,
    );

    let mut scene = scene();
    let clip = scene.clips.only(ClipLink::rect(rect(0.0, 0.0, 30.0, 30.0)));
    let params = scene
        .shader_params
        .intern(ShaderParams::of(&color_bytes([1.0, 1.0, 1.0, 1.0])));
    scene.push_shaded(ShadedQuad::new(rect(0.0, 0.0, 60.0, 60.0), id, params).clipped(clip));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    assert_eq!(pixels.rgba(15, 15)[3], 255, "inside the clip is drawn");
    assert_eq!(pixels.rgba(45, 45)[3], 0, "outside the clip is not");
}

#[test]
fn a_coverage_effect_shapes_a_box_the_ordinary_paints_fill() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-coverage",
        ShaderMode::Coverage,
        COVERAGE_SOURCE,
        SPLIT_LAYOUT,
        &COVERAGE_UNIT,
    );

    let mut scene = scene();
    let fill = scene
        .paints
        .add(zgui_scene::Paint::Solid(support::opaque(255, 0, 0)));
    let params = scene
        .shader_params
        .intern(ShaderParams::of(&0.5f32.to_ne_bytes()));
    scene.push_shaded(ShadedQuad::new(rect(0.0, 0.0, 60.0, 60.0), id, params).with_fill(fill));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    let left = pixels.rgba(10, 30);
    assert_eq!(left[3], 255, "the covered half takes the fill");
    assert!(left[0] > 200, "and it is the fill's colour: {left:?}");
    assert_eq!(
        pixels.rgba(50, 30)[3],
        0,
        "the half the effect does not cover is not drawn"
    );
}

/// An effect that cannot be drawn is reported rather than being quietly skipped.
///
/// The reasons are all recoverable, so this is a warning and not a failure — but an effect that
/// stops appearing has to say so, or it looks exactly like one that decided to draw nothing. This
/// asserts the report happens once and not once per frame.
#[test]
fn an_effect_that_cannot_be_drawn_is_reported_once() {
    let effects = zgui_render_wgpu::Effects::new();
    // Reported the first time, and silent afterwards, which is what keeps a persistent fault from
    // being a line per frame for as long as it lasts.
    for _ in 0..4 {
        effects.note_undrawable(ShaderId(1), "no pipeline");
    }
    // A different reason, and a different effect, are each their own report.
    effects.note_undrawable(ShaderId(1), "no parameters");
    effects.note_undrawable(ShaderId(2), "no pipeline");
    assert_eq!(effects.reported_count(), 3);
}

#[test]
fn an_effect_the_renderer_was_never_told_about_draws_nothing() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = scene();
    let params = scene
        .shader_params
        .intern(ShaderParams::of(&color_bytes([1.0, 1.0, 1.0, 1.0])));
    // A handle far past anything declared: a display list built against an effect this device
    // never accepted.
    scene.push_shaded(ShadedQuad::new(
        rect(0.0, 0.0, 60.0, 60.0),
        ShaderId(9_999),
        params,
    ));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    assert_eq!(pixels.rgba(30, 30)[3], 0, "nothing is drawn");
}

/// An effect that reads the content and swaps two of its channels.
const FILTER_SOURCE: &str = r#"
struct Params {
    shift: f32,
}

fn apply(
    in: ShaderInput,
    params: Params,
    beneath: texture_2d<f32>,
    beneath_sampler: sampler,
    region: FilterSource,
) -> vec4<f32> {
    // Read displaced, so the test can tell a filter that samples where it was told from one that
    // samples where it happens to be.
    let read = source_at(beneath, beneath_sampler, region, in.local - vec2<f32>(params.shift, 0.0));
    return vec4<f32>(read.b, read.g, read.r, read.a);
}
"#;

static FILTER_UNIT: OnceLock<String> = OnceLock::new();

#[test]
fn a_filter_effect_reads_the_content_of_the_group_it_is_a_step_of() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-filter",
        ShaderMode::Filter,
        FILTER_SOURCE,
        SHIFT_LAYOUT,
        &FILTER_UNIT,
    );

    let mut scene = scene();
    let params = scene
        .shader_params
        .intern(ShaderParams::of(&0.0f32.to_ne_bytes()));
    let bounds = rect(0.0, 0.0, 60.0, 60.0);
    let boundary = zgui_scene::GroupBoundary::start(
        bounds,
        1.0,
        zgui_scene::peniko::BlendMode::default(),
        [zgui_scene::Filter::Custom {
            shader: id,
            params,
            reach: 0.0,
        }]
        .into_iter()
        .collect(),
    );
    scene.push_group(boundary.clone());
    let red = scene
        .paints
        .add(zgui_scene::Paint::Solid(support::opaque(255, 0, 0)));
    scene.push_quad(zgui_scene::Quad::filled(bounds, red));
    scene.push_group(boundary.end());
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    // The effect swapped red and blue, so red content composites as blue.
    let inside = pixels.rgba(30, 30);
    assert!(
        inside[2] > 200 && inside[0] < 40,
        "the effect read the content and wrote what replaces it: {inside:?}"
    );
    assert_eq!(inside[3], 255, "and it is still opaque");
}

/// A filter that samples away from the pixel it writes reads where it was told, not where it is.
#[test]
fn a_filter_effect_samples_the_point_it_asked_for() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-filter-shift",
        ShaderMode::Filter,
        FILTER_SOURCE,
        SHIFT_LAYOUT,
        &FILTER_UNIT,
    );

    let mut scene = scene();
    // Twenty device pixels to the left, so the right half reads the left half's colour.
    let params = scene
        .shader_params
        .intern(ShaderParams::of(&20.0f32.to_ne_bytes()));
    let bounds = rect(0.0, 0.0, 60.0, 60.0);
    let boundary = zgui_scene::GroupBoundary::start(
        bounds,
        1.0,
        zgui_scene::peniko::BlendMode::default(),
        [zgui_scene::Filter::Custom {
            shader: id,
            params,
            reach: 0.0,
        }]
        .into_iter()
        .collect(),
    );
    scene.push_group(boundary.clone());
    let red = scene
        .paints
        .add(zgui_scene::Paint::Solid(support::opaque(255, 0, 0)));
    // Only the left third is painted, so anything the right two thirds shows was sampled from it.
    scene.push_quad(zgui_scene::Quad::filled(rect(0.0, 0.0, 20.0, 60.0), red));
    scene.push_group(boundary.end());
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    let shifted = pixels.rgba(30, 30);
    assert!(
        shifted[2] > 200,
        "a point twenty pixels right of the paint shows what the paint was: {shifted:?}"
    );
}

/// Drawing the same effect twice is the same picture twice.
///
/// A shaded rectangle carries two things an ordinary one does not — the pipeline its effect is
/// drawn by, and the parameter block bound beside the draw — and both are resolved per frame. A
/// second frame that resolved either differently would draw the first frame's rectangle with the
/// second frame's numbers, which for a coverage effect is a box that silently stops being drawn.
#[test]
fn an_effect_draws_the_same_picture_on_every_frame_it_is_in() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-repeat",
        ShaderMode::Coverage,
        COVERAGE_SOURCE,
        SPLIT_LAYOUT,
        &COVERAGE_UNIT,
    );

    // Ten frames, each built from scratch the way a resize drag builds them, and each interning
    // its own paints and parameters into the tables the previous frames also wrote.
    for frame in 0..10 {
        let mut scene = scene();
        let fill = scene
            .paints
            .add(zgui_scene::Paint::Solid(support::opaque(255, 0, 0)));
        let params = scene
            .shader_params
            .intern(ShaderParams::of(&0.5f32.to_ne_bytes()));
        // A different size each frame, so every frame interns a fresh entry beside the last.
        let width = 40.0 + frame as f32;
        scene.push_shaded(ShadedQuad::new(rect(0.0, 0.0, width, 60.0), id, params).with_fill(fill));
        scene.finish(&DamageSet::full());
        let pixels = present(&mut renderer, &scene);
        assert_eq!(
            pixels.rgba(5, 30)[3],
            255,
            "the covered half is drawn on frame {frame}"
        );
    }
}

/// The same across a reconfigure, which is what a resize is to a renderer.
#[test]
fn an_effect_survives_the_reconfigure_a_resize_is() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-reconfigure",
        ShaderMode::Coverage,
        COVERAGE_SOURCE,
        SPLIT_LAYOUT,
        &COVERAGE_UNIT,
    );

    for (index, scale) in [1.0f32, 2.0, 1.0, 1.5].into_iter().enumerate() {
        zgui_render::Renderer::configure(
            &mut *renderer,
            zgui_render::RenderTarget::new(
                zgui_geom::Size::new(SIDE, SIDE),
                zgui_geom::Scale::new(scale),
            ),
        );
        let mut scene = Scene::new();
        scene.begin_frame(zgui_geom::Size::new(SIDE, SIDE));
        let fill = scene
            .paints
            .add(zgui_scene::Paint::Solid(support::opaque(255, 0, 0)));
        let params = scene
            .shader_params
            .intern(ShaderParams::of(&0.5f32.to_ne_bytes()));
        scene.push_shaded(ShadedQuad::new(rect(0.0, 0.0, 60.0, 60.0), id, params).with_fill(fill));
        scene.finish(&DamageSet::full());
        let pixels = present(&mut renderer, &scene);
        assert_eq!(
            pixels.rgba(5, 30)[3],
            255,
            "still drawn after reconfigure {index} at scale {scale}"
        );
    }
}

/// A shaded rectangle replayed out of a resident chunk draws what it drew when it was encoded.
///
/// This is the path a box takes on every frame after the first: its painting is captured once, the
/// renderer keeps the bytes on the device, and later frames point a draw at them without uploading
/// anything. What the resident bytes do *not* carry is the parameter block — that is staged per
/// frame and bound beside the draw — so a replay is where the two halves of an effect can come
/// apart.
#[test]
fn a_shaded_rectangle_replayed_from_a_resident_chunk_draws_what_it_drew() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-resident",
        ShaderMode::Coverage,
        COVERAGE_SOURCE,
        SPLIT_LAYOUT,
        &COVERAGE_UNIT,
    );

    // Frame one: the painting is captured, noted, and drawn — the renderer makes it resident.
    let mut scene = scene();
    let fill = scene
        .paints
        .add(zgui_scene::Paint::Solid(support::opaque(255, 0, 0)));
    let params = scene
        .shader_params
        .intern(ShaderParams::of(&0.5f32.to_ne_bytes()));
    scene.begin_chunk_capture(zgui_scene::ChunkPrims::default());
    scene.push_shaded(ShadedQuad::new(rect(0.0, 0.0, 60.0, 60.0), id, params).with_fill(fill));
    let chunk = std::sync::Arc::new(scene.take_chunk_capture());
    scene.note_chunk_inserted(1, std::sync::Arc::clone(&chunk));
    scene.bind_capture(1);
    scene.finish(&DamageSet::full());
    let first = present(&mut renderer, &scene);
    scene.clear_chunk_notes();
    assert_eq!(
        first.rgba(5, 30)[3],
        255,
        "drawn on the frame it was encoded"
    );

    // Every frame after: the same painting, replayed out of the resident chunk.
    for frame in 0..4 {
        scene.begin_frame(zgui_geom::Size::new(SIDE, SIDE));
        scene.replay_chunk(&chunk, zgui_geom::Size::default(), 1);
        scene.finish(&DamageSet::full());
        let replayed = present(&mut renderer, &scene);
        assert_eq!(
            replayed.max_difference(&first),
            0,
            "the resident bytes draw exactly what the encoded frame drew, on replay {frame}"
        );
    }
}

/// The same, for a chunk that merely moved: the resident bytes stay put and the shift is applied
/// where the frame's offsets say, which for an effect has to move its own coordinates too.
#[test]
fn a_shaded_rectangle_replayed_at_an_offset_moves_with_its_chunk() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-moved",
        ShaderMode::Coverage,
        COVERAGE_SOURCE,
        SPLIT_LAYOUT,
        &COVERAGE_UNIT,
    );

    let mut scene = scene();
    let fill = scene
        .paints
        .add(zgui_scene::Paint::Solid(support::opaque(255, 0, 0)));
    let params = scene
        .shader_params
        .intern(ShaderParams::of(&0.5f32.to_ne_bytes()));
    scene.begin_chunk_capture(zgui_scene::ChunkPrims::default());
    scene.push_shaded(ShadedQuad::new(rect(0.0, 0.0, 40.0, 40.0), id, params).with_fill(fill));
    let chunk = std::sync::Arc::new(scene.take_chunk_capture());
    scene.note_chunk_inserted(1, std::sync::Arc::clone(&chunk));
    scene.bind_capture(1);
    scene.finish(&DamageSet::full());
    let _ = present(&mut renderer, &scene);
    scene.clear_chunk_notes();

    // Moved twenty pixels down and across.
    let by = zgui_geom::Size::new(zgui_geom::DevicePx(20.0), zgui_geom::DevicePx(20.0));
    scene.begin_frame(zgui_geom::Size::new(SIDE, SIDE));
    scene.replay_chunk(&chunk, by, 1);
    scene.finish(&DamageSet::full());
    let moved = present(&mut renderer, &scene);

    // The covered half is the box's own left half, so it moved with the box rather than staying
    // where the box used to be.
    assert_eq!(moved.rgba(25, 45)[3], 255, "inside the moved covered half");
    assert_eq!(
        moved.rgba(5, 5)[3],
        0,
        "and nothing where the box used to be"
    );
}

/// A backdrop reads what was already drawn beneath it, which is the other half of the filter seam
/// and the one a lens over a page uses.
#[test]
fn a_backdrop_effect_reads_the_composite_beneath_it() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-backdrop",
        ShaderMode::Filter,
        FILTER_SOURCE,
        SHIFT_LAYOUT,
        &FILTER_UNIT,
    );

    let mut scene = scene();
    // A red field first, which is what the backdrop will read.
    let red = scene
        .paints
        .add(zgui_scene::Paint::Solid(support::opaque(255, 0, 0)));
    scene.push_quad(zgui_scene::Quad::filled(rect(0.0, 0.0, 80.0, 80.0), red));

    // Then a box over part of it, reading it back through the effect — which swaps red and blue.
    let params = scene
        .shader_params
        .intern(ShaderParams::of(&0.0f32.to_ne_bytes()));
    let mut backdrop = zgui_scene::BackdropFilter::new(
        rect(0.0, 0.0, 40.0, 80.0),
        [zgui_scene::Filter::Custom {
            shader: id,
            params,
            reach: 0.0,
        }]
        .into_iter()
        .collect(),
    );
    backdrop.order = 0;
    scene.push_backdrop(backdrop);
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    let filtered = pixels.rgba(20, 40);
    assert!(
        filtered[2] > 200 && filtered[0] < 40,
        "the half under the backdrop was read and rewritten: {filtered:?}"
    );
    let untouched = pixels.rgba(60, 40);
    assert!(
        untouched[0] > 200 && untouched[2] < 40,
        "and the half beside it is the red that was drawn: {untouched:?}"
    );
}

/// A resident chunk holding an effect, replayed across the reconfigure a resize is.
///
/// The two halves have been tested apart — a chunk replays what it encoded, and an effect survives
/// a reconfigure — and never together. A resize is exactly both at once: the surface is
/// reconfigured, and the boxes that did not change are replayed out of the arena rather than
/// re-encoded.
#[test]
fn a_resident_effect_survives_a_reconfigure_and_keeps_drawing() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let id = declare(
        "test-resident-resize",
        ShaderMode::Coverage,
        COVERAGE_SOURCE,
        SPLIT_LAYOUT,
        &COVERAGE_UNIT,
    );

    let mut scene = scene();
    let fill = scene
        .paints
        .add(zgui_scene::Paint::Solid(support::opaque(255, 0, 0)));
    let params = scene
        .shader_params
        .intern(ShaderParams::of(&0.5f32.to_ne_bytes()));
    scene.begin_chunk_capture(zgui_scene::ChunkPrims::default());
    scene.push_shaded(ShadedQuad::new(rect(0.0, 0.0, 60.0, 60.0), id, params).with_fill(fill));
    let chunk = std::sync::Arc::new(scene.take_chunk_capture());
    scene.note_chunk_inserted(1, std::sync::Arc::clone(&chunk));
    scene.bind_capture(1);
    scene.finish(&DamageSet::full());
    let first = present(&mut renderer, &scene);
    scene.clear_chunk_notes();
    assert_eq!(first.rgba(5, 30)[3], 255, "drawn on the encoding frame");

    // A resize, then the same painting replayed out of the chunk the previous device state made
    // resident.
    for (index, scale) in [1.0f32, 2.0, 1.0].into_iter().enumerate() {
        zgui_render::Renderer::configure(
            &mut *renderer,
            zgui_render::RenderTarget::new(
                zgui_geom::Size::new(SIDE, SIDE),
                zgui_geom::Scale::new(scale),
            ),
        );
        scene.begin_frame(zgui_geom::Size::new(SIDE, SIDE));
        scene.replay_chunk(&chunk, zgui_geom::Size::default(), 1);
        scene.finish(&DamageSet::full());
        let after = present(&mut renderer, &scene);
        assert_eq!(
            after.rgba(5, 30)[3],
            255,
            "still drawn after reconfigure {index} at scale {scale}"
        );
    }
}
