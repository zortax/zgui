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

mod common;

use zgui_platform_headless::Harness;
use zgui_runtime::Runtime;
use zgui_shader::{NoParams, ShaderEffect, ShaderParams, shader};
use zgui_testkit_compositor::Instance;
use zgui_view::{BuildCx, IntoView, View};

use crate::common::Seen;

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

/// An application whose window draws one card through a compositor onto a screen that records.
fn app(css: &str) -> (Harness<Runtime>, Seen) {
    common::app(css, move |cx: &mut BuildCx<'_>| {
        Box::new(zgui_elements::r#box().class("card").into_view().build(cx))
    })
}

/// The window's shader quads and the quads its compositor draws.
fn drawn(seen: &Seen) -> (Vec<Instance>, Vec<Instance>) {
    (seen.shader_quads(), seen.quads())
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
    let (mut app, seen) = app(&sheet(""));
    app.settle(16);

    let (shaded, quads) = drawn(&seen);
    assert!(shaded.is_empty(), "no effect was named");
    assert_eq!(quads.len(), 1, "the background is one quad");
}

#[test]
fn a_corner_shape_replaces_the_quad_and_keeps_the_paints_that_fill_it() {
    let _ = SQUIRCLE.register();
    let (mut app, seen) = app(&sheet(
        "--zgui-shape: style-shape; --style-shape-exponent: 4",
    ));
    app.settle(16);

    let (shaded, quads) = drawn(&seen);
    assert_eq!(shaded.len(), 1, "the box is drawn by the effect");
    assert!(quads.is_empty(), "and not by an ordinary quad as well");
    assert_eq!(
        shaded[0].color("fill"),
        Some([1.0, 0.0, 0.0, 1.0]),
        "a coverage effect shapes the box the cascade's own background fills: {}",
        shaded[0].text()
    );
}

/// The case a background *colour* does not cover: a shaped box is filled by the whole cascade,
/// and a gradient is the commonest thing a card is filled with.
#[test]
fn a_corner_shape_keeps_a_gradient_the_way_it_keeps_a_colour() {
    let _ = SQUIRCLE.register();
    let (mut app, seen) = app("box { display: block; width: 120px; height: 60px; \
         background: linear-gradient(140deg, #8fc0ff, #6ea8ff); \
         --zgui-shape: style-shape; --style-shape-exponent: 4 }");
    app.settle(16);

    let (shaded, quads) = drawn(&seen);
    assert_eq!(shaded.len(), 1, "the box is drawn by the effect");
    assert!(quads.is_empty());
    assert_eq!(
        shaded[0].paint("fill").as_deref(),
        Some("linear"),
        "the shape is the effect's and the ramp that fills it is the sheet's: {}",
        shaded[0].text()
    );
}

/// A ramp is resolved against the box at the origin, so a shaped box that has moved has to sample
/// where it *was* — exactly as an ordinary one does.
#[test]
fn a_shaped_box_carries_the_origin_its_ramp_was_resolved_against() {
    let _ = SQUIRCLE.register();
    let (mut app, seen) = app("root { display: block; padding: 30px } \
         box { display: block; width: 120px; height: 60px; \
         background: linear-gradient(140deg, #8fc0ff, #6ea8ff); \
         --zgui-shape: style-shape }");
    app.settle(16);

    let (shaded, _) = drawn(&seen);
    let [x, y, ..] = shaded[0].bounds();
    assert_eq!(
        shaded[0].field("paint_origin"),
        Some(format!("[{x}, {y}]").as_str()),
        "the ramp travels with the rectangle it fills: {}",
        shaded[0].text()
    );
}

#[test]
fn a_parameter_written_beside_the_name_reaches_the_block() {
    let _ = SQUIRCLE.register();
    let (mut app, seen) = app(&sheet(
        "--zgui-shape: style-shape; --style-shape-exponent: 4",
    ));
    app.settle(16);

    let params = seen.params_of(&seen.shader_quads()[0]);
    assert_eq!(
        &params.user[0..4],
        &4.0f32.to_ne_bytes(),
        "the exponent the sheet wrote is where the effect declares it"
    );
}

#[test]
fn a_paint_effect_fills_the_box_in_place_of_its_background() {
    let _ = WASH.register();
    let (mut app, seen) = app(&sheet("--zgui-shader: style-wash"));
    app.settle(16);

    let (shaded, quads) = drawn(&seen);
    assert_eq!(shaded.len(), 1, "the box is drawn by the effect");
    assert!(quads.is_empty());
    assert_eq!(
        shaded[0].field("fill"),
        None,
        "a paint effect shades the whole box itself: {}",
        shaded[0].text()
    );
}

#[test]
fn a_name_nothing_declared_leaves_the_box_as_it_was() {
    let (mut app, seen) = app(&sheet("--zgui-shape: never-declared"));
    app.settle(16);

    let (shaded, quads) = drawn(&seen);
    assert!(shaded.is_empty(), "a misspelt name draws no effect");
    assert_eq!(quads.len(), 1, "and the box keeps the painting it had");
}

/// The property says what the effect is expected to do, and an effect that does something else is
/// refused rather than drawn: a colour where a shape was asked for is a wrong picture.
#[test]
fn an_effect_named_by_the_wrong_property_is_refused() {
    let _ = WASH.register();
    let (mut app, seen) = app(&sheet("--zgui-shape: style-wash"));
    app.settle(16);

    let (shaded, quads) = drawn(&seen);
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

/// A sheet that puts the card 30 pixels in from the window's corner.
fn inset(extra: &str) -> String {
    format!("root {{ display: block; padding: 30px }} {}", sheet(extra))
}

/// The groups the window's compositor opens.
fn groups(seen: &Seen) -> Vec<Instance> {
    seen.drawing().objects("group_begin").cloned().collect()
}

#[test]
fn a_filter_effect_becomes_a_step_of_the_group_the_box_is_composited_through() {
    let _ = GLASS.register();
    let (mut app, seen) = app(&inset("--zgui-filter: style-glass"));
    app.settle(16);

    let opened = groups(&seen);
    assert_eq!(opened.len(), 1, "a filtered box is composited on its own");
    let filters = opened[0].field("filters").unwrap_or_default();
    assert_eq!(
        filters.matches("shader(").count(),
        1,
        "and the effect is one step of its chain: {}",
        opened[0].text()
    );
}

/// The whole reason an effect declares how far it reads: the group has to be told to read outside
/// what it writes, or a partial redraw feeds the filter its own previous output.
#[test]
fn the_reach_an_effect_declared_reaches_the_group_that_runs_it() {
    let _ = GLASS.register();
    let (mut app, seen) = app(&inset("--zgui-filter: style-glass"));
    app.settle(16);

    let opened = groups(&seen);
    assert!(
        opened[0]
            .field("filters")
            .is_some_and(|filters| filters.contains("reach=6")),
        "the step reads as far as the effect declared: {}",
        opened[0].text()
    );
    assert!(
        opened[0].bounds()[0] < 30.0,
        "and the group's region reaches outside the box at x = 30: {}",
        opened[0].text()
    );
}

/// A backdrop filter reads what was drawn *beneath* the box rather than what the box drew, so it
/// is drawn as a backdrop rather than as a step of the box's own chain.
#[test]
fn a_backdrop_effect_becomes_a_backdrop_the_box_reads_through() {
    let _ = GLASS.register();
    let (mut app, seen) = app(&inset("--zgui-backdrop-filter: style-glass"));
    app.settle(16);

    let backdrops: Vec<Instance> = seen.drawing().objects("backdrop").cloned().collect();
    assert_eq!(backdrops.len(), 1, "the box reads what is beneath it");
    assert!(
        backdrops[0]
            .field("filters")
            .is_some_and(|filters| filters.matches("shader(").count() == 1),
        "through the effect the sheet named: {}",
        backdrops[0].text()
    );
}

/// A backdrop that samples a neighbourhood has to say how far, or a partial redraw feeds it its
/// own previous output and the panel smears further every frame.
#[test]
fn a_backdrop_effect_reads_as_far_outside_itself_as_it_declared() {
    let _ = GLASS.register();
    let (mut app, seen) = app(&inset("--zgui-backdrop-filter: style-glass"));
    app.settle(16);

    let backdrops: Vec<Instance> = seen.drawing().objects("backdrop").cloned().collect();
    let read = backdrops[0].rect("source").unwrap_or_else(|| {
        panic!(
            "the backdrop reads only what it writes: {}",
            backdrops[0].text()
        )
    });
    assert!(
        read[0] < backdrops[0].bounds()[0],
        "this effect declared a reach, so it reads outside its own rectangle: {}",
        backdrops[0].text()
    );
}

/// The property says the effect filters, and an effect that draws a rectangle is refused.
#[test]
fn a_rectangle_effect_named_as_a_filter_is_refused() {
    let _ = WASH.register();
    let (mut app, seen) = app(&inset("--zgui-filter: style-wash"));
    app.settle(16);

    let opened = groups(&seen);
    assert!(
        opened.iter().all(|group| !group.text().contains("shader(")),
        "nothing filters: {opened:?}"
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
    let (mut app, seen) = app(&sheet("--zgui-shader: style-follow"));
    app.settle(16);

    let before = seen.params_of(&seen.shader_quads()[0]);
    // Nothing has moved a pointer over this window, so the effect is told there is none.
    assert_eq!(before.hovered, 0.0);
    assert_eq!(before.pointer, [0.0, 0.0]);
}

/// The whole point of declaring the read: a box that does not is drawn identically wherever the
/// pointer is, so a pointer stream over an ordinary document repaints nothing.
#[test]
fn an_effect_that_declared_nothing_carries_no_pointer_at_all() {
    let _ = WASH.register();
    let (mut app, seen) = app(&sheet("--zgui-shader: style-wash"));
    app.settle(16);

    let params = seen.params_of(&seen.shader_quads()[0]);
    assert_eq!(params.hovered, 0.0);
    assert_eq!(params.pointer, [0.0, 0.0]);
}

/// These properties inherit, because every unregistered custom property does. A subtree that does
/// not want its ancestor's shape says so, and `none` is how.
#[test]
fn a_subtree_opts_out_of_a_shape_it_inherited() {
    let _ = SQUIRCLE.register();
    let (mut app, seen) = app("root { display: block; --zgui-shape: style-shape } \
         box { display: block; width: 120px; height: 60px; background-color: #ff0000; \
         --zgui-shape: none }");
    app.settle(16);

    let (shaded, quads) = drawn(&seen);
    assert!(shaded.is_empty(), "the box refused the shape it inherited");
    assert_eq!(quads.len(), 1, "and is drawn the way it would have been");
}

/// A paint effect that reads the clock.
static PULSE: ShaderEffect<NoParams> = shader! {
    name: "style-pulse",
    mode: Paint,
    reads: [Time],
    source: r#"
        fn shade(in: ShaderInput, params: Params) -> vec4<f32> {
            return premultiplied(vec3<f32>(1.0), fract(in.time));
        }
    "#,
};

/// An effect that reads the clock has to be told what time it is, and is drawn again on every
/// frame while it is shown.
///
/// Nothing the framework draws reads the frame clock, so it can be absent for a long time without
/// anything looking wrong: an effect that breathes simply does not breathe. This is the assertion
/// that says the clock arrives.
#[test]
fn the_frame_clock_reaches_an_effect_that_reads_it() {
    let _ = PULSE.register();
    let (mut app, seen) = app(&sheet("--zgui-shader: style-pulse"));
    app.settle(8);
    let first = seen.clocks().len();

    app.advance(std::time::Duration::from_millis(250));
    app.app_mut().windows_mut()[0].request_frame();
    app.settle(8);
    let clocks = seen.clocks();
    assert!(
        clocks.len() > first,
        "a frame that changed nothing is drawn again for an effect that reads the clock"
    );
    let (earlier, later) = (clocks[first - 1], clocks[clocks.len() - 1]);
    // The compositor's clock is the window's frame moment, which only the harness moves.
    assert!(
        later.time - earlier.time >= 0.25,
        "and the clock an effect reads moves with the window's own: {} then {}",
        earlier.time,
        later.time
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
    // A width that follows the window's, so every resize lays the box out and paints it again.
    let (mut app, seen) = app(&sheet(
        "--zgui-shape: style-shape; --style-shape-exponent: 4; width: 15%",
    ));
    app.settle(16);
    assert_eq!(drawn(&seen).0.len(), 1, "drawn before any resize");

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
        let (shaded, quads) = drawn(&seen);
        assert_eq!(shaded.len(), 1, "still drawn by the effect at {width}");
        assert!(quads.is_empty(), "and never by an ordinary quad at {width}");
    }
}

/// The same across a scale change, which reallocates every target the frame is composed into.
#[test]
fn a_shaped_box_is_still_shaped_after_the_scale_factor_changes() {
    let _ = SQUIRCLE.register();
    let (mut app, seen) = app(&sheet("--zgui-shape: style-shape"));
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
        assert_eq!(drawn(&seen).0.len(), 1, "still drawn at scale {scale}");
    }
}
