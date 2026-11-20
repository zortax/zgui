//! A shader handle whose parameters change every frame costs a parameter write per frame.
//!
//! The counters are process-wide, so this binary holds one test.

// The root a `shader!` expansion names its crates through.
extern crate zgui_shader as zgui;

mod common;

use zgui_custom::{CustomElement, CustomLayoutCx, CustomMeasured, ScenePainter};
use zgui_geom::{DevicePx, Point, Rect, Size};
use zgui_profile::{Counter, counter};
use zgui_shader::{NoParams, ShaderEffect, ShaderHandle, ShaderPainterExt, shader};
use zgui_view::{BuildCx, IntoView, View};

/// An effect that follows the pointer it is told about.
static FOLLOW: ShaderEffect<NoParams> = shader! {
    name: "params-follow",
    mode: Paint,
    reads: [Pointer],
    source: r#"
        fn shade(in: ShaderInput, params: Params) -> vec4<f32> {
            let near = exp(-distance(in.local, in.pointer) / 20.0);
            return premultiplied(vec3<f32>(1.0), near * in.hovered);
        }
    "#,
};

/// A pane that fills its content box with the effect.
struct Pane {
    handle: ShaderHandle<NoParams>,
}

impl CustomElement for Pane {
    fn layout(&mut self, _cx: &mut CustomLayoutCx<'_>) -> CustomMeasured {
        CustomMeasured {
            width: 120.0,
            height: 40.0,
            ..CustomMeasured::default()
        }
    }

    fn paint(&mut self, painter: &mut ScenePainter<'_>) {
        let size = painter.size();
        let whole = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(size.width, size.height),
        );
        painter.effect(whole, 0.0, &self.handle);
    }
}

/// How many frames the pointer moves for.
const FRAMES: usize = 12;

#[test]
fn a_pointer_that_moves_every_frame_writes_the_parameters_and_records_no_chunk() {
    let follow = FOLLOW.register();
    let (view, _element) = zgui_custom::custom(Pane {
        handle: follow.clone(),
    });
    let mut view = Some(view);
    let (mut app, seen) = common::app("root { display: block }", move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::r#box()
                .class("root")
                .child(view.take().expect("built once"))
                .into_view()
                .build(cx),
        )
    });
    app.settle(16);
    let recorded = counter::get(Counter::ChunksReencoded);
    let written = seen.params_written();

    for step in 1..=FRAMES {
        follow.set_pointer([step as f32, 10.0], true);
        app.settle(4);
    }

    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(
            counter::get(Counter::ChunksReencoded) - recorded,
            0,
            "the pane was not recorded again"
        );
    }
    assert_eq!(
        seen.params_written() - written,
        FRAMES,
        "each move is one parameter write"
    );
    let drawn = seen.shader_quads();
    assert_eq!(
        seen.params_of(&drawn[0]).pointer,
        [FRAMES as f32, 10.0],
        "and the shader quad draws the last one"
    );
}
