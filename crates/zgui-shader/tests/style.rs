//! Attaching an effect from a style sheet, through the real frame loop.
//!
//! Two properties, two things they do. `--zgui-shape` names a coverage effect, and the box
//! keeps its background and its border — the effect decides only which pixels are inside.
//! `--zgui-shader` names a paint effect, which fills the box in place of its background. Both are
//! custom properties, because there is no `@property` registration in this build and a custom
//! property's computed value is a token stream — which is what makes it the non-forking way to
//! feed the engine something it has no property for.

// The root a `shader!` expansion names its crates through.
extern crate zgui_shader as zgui;

use std::sync::Arc;

use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_runtime::{App, AppError, Runtime};
use zgui_shader::{NoParams, ShaderEffect, ShaderParams, shader};
use zgui_view::{Anchor, BuildCx, IntoView, View};

/// What the smoothing effect draws with.
#[repr(C)]
#[derive(Clone, Copy, Default, ShaderParams)]
struct Smoothing {
    /// The superellipse's exponent.
    exponent: f32,
}

/// A coverage effect, which reshapes a box the ordinary paints fill.
static SQUIRCLE: ShaderEffect<Smoothing> = shader! {
    name: "style-shape",
    mode: Coverage,
    params: Smoothing,
    source: r#"
        struct Params {
            exponent: f32,
        }

        fn coverage(in: ShaderInput, params: Params) -> f32 {
            let half = in.size * 0.5;
            return coverage_of(superellipse_sdf(in.local - half, half, params.exponent));
        }
    "#,
};

/// A paint effect, which fills a box in place of its background.
static WASH: ShaderEffect<NoParams> = shader! {
    name: "style-wash",
    mode: Paint,
    source: r#"
        fn shade(in: ShaderInput, params: Params) -> vec4<f32> {
            return premultiplied(vec3<f32>(0.0, 1.0, 0.0), in.uv.x);
        }
    "#,
};

/// An application whose window records rather than draws.
fn app(css: &str) -> Harness<Runtime> {
    let handler = App::new()
        .with_title("style")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_renderer(Box::new(
            |_surface: &Arc<dyn Surface>, target| -> Result<_, AppError> {
                let mut renderer = zgui_testkit_scene::CaptureRenderer::new().shading();
                zgui_render::Renderer::configure(&mut renderer, target);
                Ok(Box::new(renderer) as Box<dyn zgui_render::Renderer>)
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::r#box()
                    .class("card")
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    Harness::new(handler)
}

/// The window's shaded rectangles and its ordinary ones.
fn drawn(app: &mut Harness<Runtime>) -> (Vec<zgui_scene::ShadedQuad>, Vec<zgui_scene::Quad>) {
    let scene = app.app_mut().windows_mut()[0].scene();
    (
        scene.primitives.shaded.clone(),
        scene.primitives.quads.clone(),
    )
}

/// A sheet for a card of the given declarations.
fn sheet(extra: &str) -> String {
    format!(
        "box {{ display: block; width: 120px; height: 60px; \
         background-color: #ff0000; {extra} }}"
    )
}

#[test]
fn an_ordinary_box_draws_an_ordinary_quad() {
    // Registered so the declarations exist; the sheet below names neither.
    let _ = SQUIRCLE.register();
    let mut app = app(&sheet(""));
    app.settle(16);

    let (shaded, quads) = drawn(&mut app);
    assert!(shaded.is_empty(), "no effect was named");
    assert_eq!(quads.len(), 1, "the background is one quad");
}

#[test]
fn a_corner_shape_replaces_the_quad_and_keeps_the_paints_that_fill_it() {
    let _ = SQUIRCLE.register();
    let mut app = app(&sheet("--zgui-shape: style-shape; --style-shape-exponent: 4"));
    app.settle(16);

    let (shaded, quads) = drawn(&mut app);
    assert_eq!(shaded.len(), 1, "the box is drawn by the effect");
    assert!(quads.is_empty(), "and not by an ordinary quad as well");
    assert_ne!(
        shaded[0].fill,
        zgui_scene::PaintRef::NONE,
        "a coverage effect shapes the box the cascade's own background fills"
    );
}

/// The case a background *colour* does not cover: a shaped box is filled by the whole cascade,
/// and a gradient is the commonest thing a card is filled with.
#[test]
fn a_corner_shape_keeps_a_gradient_the_way_it_keeps_a_colour() {
    let _ = SQUIRCLE.register();
    let mut app = app(
        "box { display: block; width: 120px; height: 60px; \
         background: linear-gradient(140deg, #8fc0ff, #6ea8ff); \
         --zgui-shape: style-shape; --style-shape-exponent: 4 }",
    );
    app.settle(16);

    let (shaded, quads) = drawn(&mut app);
    assert_eq!(shaded.len(), 1, "the box is drawn by the effect");
    assert!(quads.is_empty());
    let scene = app.app_mut().windows_mut()[0].scene();
    let fill = scene
        .paints
        .get(zgui_scene::PaintId(shaded[0].fill.index))
        .expect("the gradient interned");
    assert!(
        matches!(fill, zgui_scene::Paint::Gradient { .. }),
        "the shape is the effect's and the ramp that fills it is the sheet's: {fill:?}"
    );
}

/// A ramp is resolved against the box at the origin, so a shaped box that has moved has to sample
/// where it *was* — exactly as an ordinary one does.
#[test]
fn a_shaped_box_carries_the_origin_its_ramp_was_resolved_against() {
    let _ = SQUIRCLE.register();
    let mut app = app(
        "root { display: block; padding: 30px } \
         box { display: block; width: 120px; height: 60px; \
         background: linear-gradient(140deg, #8fc0ff, #6ea8ff); \
         --zgui-shape: style-shape }",
    );
    app.settle(16);

    let (shaded, _) = drawn(&mut app);
    assert!(shaded[0].samples_its_paint());
    assert_eq!(
        (shaded[0].paint_origin[0], shaded[0].paint_origin[1]),
        (shaded[0].bounds[0], shaded[0].bounds[1]),
        "the ramp travels with the rectangle it fills"
    );
}

#[test]
fn a_parameter_written_beside_the_name_reaches_the_block() {
    let _ = SQUIRCLE.register();
    let mut app = app(&sheet("--zgui-shape: style-shape; --style-shape-exponent: 4"));
    app.settle(16);
    let scene = app.app_mut().windows_mut()[0].scene();
    let slot = scene.primitives.shaded[0].params_slot();
    let params = scene.shader_params.get(slot).expect("the block interned");
    assert_eq!(
        &params.user[0..4],
        &4.0f32.to_ne_bytes(),
        "the exponent the sheet wrote is where the effect declares it"
    );
}

#[test]
fn a_paint_effect_fills_the_box_in_place_of_its_background() {
    let _ = WASH.register();
    let mut app = app(&sheet("--zgui-shader: style-wash"));
    app.settle(16);

    let (shaded, quads) = drawn(&mut app);
    assert_eq!(shaded.len(), 1, "the box is drawn by the effect");
    assert!(quads.is_empty());
    assert_eq!(
        shaded[0].fill,
        zgui_scene::PaintRef::NONE,
        "a paint effect shades the whole box itself"
    );
}

#[test]
fn a_name_nothing_declared_leaves_the_box_as_it_was() {
    let mut app = app(&sheet("--zgui-shape: never-declared"));
    app.settle(16);

    let (shaded, quads) = drawn(&mut app);
    assert!(shaded.is_empty(), "a misspelt name draws no effect");
    assert_eq!(quads.len(), 1, "and the box keeps the painting it had");
}

/// The property says what the effect is expected to do, and an effect that does something else is
/// refused rather than drawn: a colour where a shape was asked for is a wrong picture.
#[test]
fn an_effect_named_by_the_wrong_property_is_refused() {
    let _ = WASH.register();
    let mut app = app(&sheet("--zgui-shape: style-wash"));
    app.settle(16);

    let (shaded, quads) = drawn(&mut app);
    assert!(shaded.is_empty());
    assert_eq!(quads.len(), 1, "the box keeps the painting it had");
}

/// A filter effect, which reads the box's own content.
static GLASS: ShaderEffect<NoParams> = shader! {
    name: "style-glass",
    mode: Filter,
    reach: 6.0,
    source: r#"
        fn apply(
            in: ShaderInput,
            params: Params,
            beneath: texture_2d<f32>,
            beneath_sampler: sampler,
            region: FilterSource,
        ) -> vec4<f32> {
            return source_at(beneath, beneath_sampler, region, in.local + vec2<f32>(4.0, 0.0));
        }
    "#,
};

/// The window's group boundaries.
fn groups(app: &mut Harness<Runtime>) -> Vec<zgui_scene::GroupBoundary> {
    app.app_mut().windows_mut()[0]
        .scene()
        .primitives
        .groups
        .clone()
}

#[test]
fn a_filter_effect_becomes_a_step_of_the_group_the_box_is_composited_through() {
    let _ = GLASS.register();
    let mut app = app(&sheet("--zgui-filter: style-glass"));
    app.settle(16);

    let opened: Vec<zgui_scene::GroupBoundary> =
        groups(&mut app).into_iter().filter(|g| g.is_start).collect();
    assert_eq!(opened.len(), 1, "a filtered box is composited on its own");
    assert!(
        opened[0]
            .filters
            .iter()
            .any(|filter| matches!(filter, zgui_scene::Filter::Custom { .. })),
        "and the effect is a step of its chain: {:?}",
        opened[0].filters
    );
}

/// The whole reason an effect declares how far it reads: the group has to be told to read outside
/// what it writes, or a partial redraw feeds the filter its own previous output.
#[test]
fn the_reach_an_effect_declared_reaches_the_group_that_runs_it() {
    let _ = GLASS.register();
    let mut app = app(&sheet("--zgui-filter: style-glass"));
    app.settle(16);

    let opened: Vec<zgui_scene::GroupBoundary> =
        groups(&mut app).into_iter().filter(|g| g.is_start).collect();
    let boundary = &opened[0];
    assert!(
        boundary.source.origin.x.0 < boundary.bounds.origin.x.0,
        "the group reads outside the rectangle it writes: {:?} vs {:?}",
        boundary.source,
        boundary.bounds
    );
}

/// A backdrop filter reads what was drawn *beneath* the box rather than what the box drew, so it
/// reaches the display list as a backdrop rather than as a step of the box's own chain.
#[test]
fn a_backdrop_effect_becomes_a_backdrop_the_box_reads_through() {
    let _ = GLASS.register();
    let mut app = app(&sheet("--zgui-backdrop-filter: style-glass"));
    app.settle(16);

    let backdrops = app.app_mut().windows_mut()[0]
        .scene()
        .primitives
        .backdrops
        .clone();
    assert_eq!(backdrops.len(), 1, "the box reads what is beneath it");
    assert!(
        backdrops[0]
            .filters
            .iter()
            .any(|filter| matches!(filter, zgui_scene::Filter::Custom { .. })),
        "through the effect the sheet named: {:?}",
        backdrops[0].filters
    );
}

/// A backdrop that samples a neighbourhood has to say how far, or a partial redraw feeds it its
/// own previous output and the panel smears further every frame.
#[test]
fn a_backdrop_effect_reads_as_far_outside_itself_as_it_declared() {
    let _ = GLASS.register();
    let mut app = app(&sheet("--zgui-backdrop-filter: style-glass"));
    app.settle(16);

    let backdrops = app.app_mut().windows_mut()[0]
        .scene()
        .primitives
        .backdrops
        .clone();
    assert!(
        !backdrops[0].reads_only_what_it_writes(),
        "this effect declared a reach, so it reads outside its own rectangle"
    );
}

/// The property says the effect filters, and an effect that draws a rectangle is refused.
#[test]
fn a_rectangle_effect_named_as_a_filter_is_refused() {
    let _ = WASH.register();
    let mut app = app(&sheet("--zgui-filter: style-wash"));
    app.settle(16);

    let opened: Vec<zgui_scene::GroupBoundary> =
        groups(&mut app).into_iter().filter(|g| g.is_start).collect();
    assert!(
        opened
            .iter()
            .all(|g| !g.filters.iter().any(|f| matches!(f, zgui_scene::Filter::Custom { .. }))),
        "nothing filters"
    );
}

/// An effect that follows the pointer, which is what the declaration is for.
static FOLLOW: ShaderEffect<NoParams> = shader! {
    name: "style-follow",
    mode: Paint,
    reads: [Pointer],
    source: r#"
        fn shade(in: ShaderInput, params: Params) -> vec4<f32> {
            let near = exp(-distance(in.local, in.pointer) / 20.0);
            return premultiplied(vec3<f32>(1.0), near * in.hovered);
        }
    "#,
};

#[test]
fn an_effect_that_declared_it_reads_the_pointer_is_told_where_the_pointer_is() {
    let _ = FOLLOW.register();
    let mut app = app(&sheet("--zgui-shader: style-follow"));
    app.settle(16);
    let before = {
        let scene = app.app_mut().windows_mut()[0].scene();
        let slot = scene.primitives.shaded[0].params_slot();
        *scene.shader_params.get(slot).expect("the block interned")
    };
    // Nothing has moved a pointer over this window, so the effect is told there is none.
    assert_eq!(before.hovered, 0.0);
    assert_eq!(before.pointer, [0.0, 0.0]);
}

/// The whole point of declaring the read: a box that does not is drawn identically wherever the
/// pointer is, so a pointer stream over an ordinary document repaints nothing.
#[test]
fn an_effect_that_declared_nothing_carries_no_pointer_at_all() {
    let _ = WASH.register();
    let mut app = app(&sheet("--zgui-shader: style-wash"));
    app.settle(16);
    let scene = app.app_mut().windows_mut()[0].scene();
    let slot = scene.primitives.shaded[0].params_slot();
    let params = scene.shader_params.get(slot).expect("the block interned");
    assert_eq!(params.hovered, 0.0);
    assert_eq!(params.pointer, [0.0, 0.0]);
}

/// These properties inherit, because every unregistered custom property does. A subtree that does
/// not want its ancestor's shape says so, and `none` is how.
#[test]
fn a_subtree_opts_out_of_a_shape_it_inherited() {
    let _ = SQUIRCLE.register();
    let mut app = app(
        "root { display: block; --zgui-shape: style-shape } \
         box { display: block; width: 120px; height: 60px; background-color: #ff0000; \
         --zgui-shape: none }",
    );
    app.settle(16);

    let (shaded, quads) = drawn(&mut app);
    assert!(shaded.is_empty(), "the box refused the shape it inherited");
    assert_eq!(quads.len(), 1, "and is drawn the way it would have been");
}

/// An effect that reads the clock has to be told what time it is.
///
/// Nothing the framework draws reads the frame clock, so it can be absent for a long time without
/// anything looking wrong: an effect that breathes simply does not breathe, and reads as a static
/// gradient that follows the pointer. This is the assertion that says the clock arrives.
#[test]
fn the_frame_clock_reaches_the_display_list() {
    let _ = WASH.register();
    let mut app = app(&sheet("--zgui-shader: style-wash"));
    app.settle(8);

    let first = app.app_mut().windows_mut()[0].scene().frame_clock();
    assert!(
        first.scale > 0.0,
        "an effect is told how many device pixels one CSS pixel is: {first:?}"
    );

    // Nothing moves a virtual clock but a caller, so the time this window has been up is moved
    // here rather than waited for.
    app.advance(std::time::Duration::from_millis(250));
    app.app_mut().windows_mut()[0].request_frame();
    app.settle(8);
    let later = app.app_mut().windows_mut()[0].scene().frame_clock();
    assert!(
        later.seconds > first.seconds,
        "and the clock an effect reads moves with it: {} then {}",
        first.seconds,
        later.seconds
    );
}

/// The surface the harness opened.
fn only_surface(app: &Harness<Runtime>) -> zgui_platform::SurfaceId {
    app.platform()
        .offscreens()
        .first()
        .map(|surface| zgui_platform::Surface::id(surface.as_ref()))
        .expect("the application opened its window")
}

/// A shaped box has to survive a resize: the effect is the same effect, and the shape follows the
/// box rather than the frame it was first drawn in.
#[test]
fn a_shaped_box_is_still_shaped_after_the_window_is_resized() {
    let _ = SQUIRCLE.register();
    let mut app = app(&sheet(
        "--zgui-shape: style-shape; --style-shape-exponent: 4",
    ));
    app.settle(16);
    assert_eq!(drawn(&mut app).0.len(), 1, "drawn before any resize");

    let surface = only_surface(&app);
    for width in [820.0, 900.0, 640.0, 1000.0, 700.0] {
        app.deliver(
            surface,
            zgui_platform::SurfaceEvent::Resized(zgui_geom::Size::new(
                zgui_geom::DevicePx(width),
                zgui_geom::DevicePx(600.0),
            )),
        );
        app.settle(16);
        let (shaded, quads) = drawn(&mut app);
        assert_eq!(shaded.len(), 1, "still drawn by the effect at {width}");
        assert!(quads.is_empty(), "and never by an ordinary quad at {width}");
    }
}

/// The same across a scale change, which reallocates every target the frame is composed into.
#[test]
fn a_shaped_box_is_still_shaped_after_the_scale_factor_changes() {
    let _ = SQUIRCLE.register();
    let mut app = app(&sheet("--zgui-shape: style-shape"));
    app.settle(16);

    let surface = only_surface(&app);
    for scale in [2.0, 1.0, 1.5] {
        app.deliver(
            surface,
            zgui_platform::SurfaceEvent::ScaleFactorChanged {
                scale_factor: scale,
                size: zgui_geom::Size::new(
                    zgui_geom::DevicePx(400.0 * scale as f32),
                    zgui_geom::DevicePx(300.0 * scale as f32),
                ),
            },
        );
        app.settle(16);
        assert_eq!(drawn(&mut app).0.len(), 1, "still drawn at scale {scale}");
    }
}
