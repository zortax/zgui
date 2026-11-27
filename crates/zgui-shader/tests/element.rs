//! An effect painted by a custom element, through the real frame loop.
//!
//! This says what the compositor draws: one shader quad, positioned by CSS, carrying the effect
//! and the parameters the handle was set to. It runs with a renderer that records rather than
//! draws, so it needs no device.

mod common;

// The root a `shader!` expansion names its crates through. An application reaches it as
// `zgui::shader`; a crate with no umbrella over it supplies the root itself.
extern crate zgui_shader as zgui;

use zgui_custom::{CustomElement, CustomHandle, CustomLayoutCx, CustomMeasured, ScenePainter};
use zgui_geom::{DevicePx, Point, Rect, Size};
use zgui_platform_headless::Harness;
use zgui_runtime::Runtime;
use zgui_shader::{ShaderEffect, ShaderHandle, ShaderPainterExt, ShaderParams, shader};
use zgui_view::{BuildCx, IntoView, View};

use crate::common::{Seen, app};

/// What the fixture effect draws with.
#[repr(C)]
#[derive(Clone, Copy, Default, ShaderParams)]
struct Tint {
    /// A number the shader multiplies its output by.
    amount: f32,
}

/// An effect that shades one colour, so the test names something a device would draw.
static TINT: ShaderEffect<Tint> = shader! {
    name: "test-tint",
    mode: Paint,
    params: Tint,
    source: r#"
        struct Params {
            amount: f32,
        }

        fn shade(in: ShaderInput, params: Params) -> vec4<f32> {
            return vec4<f32>(0.0, params.amount, 0.0, params.amount);
        }
    "#,
};

/// A pane that fills its content box with the effect.
struct Pane {
    /// The effect and its parameters.
    handle: ShaderHandle<Tint>,
}

impl CustomElement for Pane {
    fn layout(&mut self, cx: &mut CustomLayoutCx<'_>) -> CustomMeasured {
        CustomMeasured {
            width: cx.known_width.unwrap_or(80.0),
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
        painter.effect(whole, 4.0, &self.handle);
    }
}

/// The fixture, and the two handles that drive it.
fn paned() -> (
    Harness<Runtime>,
    Seen,
    ShaderHandle<Tint>,
    CustomHandle<Pane>,
) {
    let tint = TINT.register();
    let (view, element) = zgui_custom::custom(Pane {
        handle: tint.clone(),
    });
    let (app, seen) = app(SHEET, {
        let mut view = Some(view);
        move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::r#box()
                    .class("root")
                    .child(view.take().expect("built once"))
                    .into_view()
                    .build(cx),
            )
        }
    });
    (app, seen, tint, element)
}

const SHEET: &str = "root { display: block; width: 400px; height: 300px; padding: 10px }";

#[test]
fn an_effect_is_drawn_as_one_shader_quad_where_css_put_it() {
    let (mut app, seen, tint, _element) = paned();
    tint.set_params(Tint { amount: 0.5 });
    app.settle(16);

    let drawn = seen.shader_quads();
    assert_eq!(drawn.len(), 1, "one effect is one shader quad: {drawn:?}");
    let [x, y, width, height] = drawn[0].bounds();
    assert_eq!(
        (x, y),
        (10.0, 10.0),
        "the effect sits at the content box the sheet's padding produced"
    );
    assert_eq!(
        (width, height),
        (380.0, 40.0),
        "at the size layout gave the element"
    );
    assert!(
        drawn[0]
            .field("radii")
            .is_some_and(|radii| radii.starts_with("[4,")),
        "and carries the corner it was drawn with: {}",
        drawn[0].text()
    );
    assert_eq!(
        &seen.params_of(&drawn[0]).user[0..4],
        &0.5f32.to_ne_bytes(),
        "with the parameters the handle was set to"
    );
}

#[test]
fn a_repaint_draws_the_parameters_the_handle_was_set_to() {
    let (mut app, seen, tint, element) = paned();
    tint.set_params(Tint { amount: 0.25 });
    app.settle(16);

    tint.set_params(Tint { amount: 0.75 });
    // The parameters changed, so the element owes a repaint: an effect that is replayed draws the
    // parameters it was painted with, which is the whole reason this is said out loud.
    element.repaint();
    app.settle(16);

    let drawn = seen.shader_quads();
    assert_eq!(
        &seen.params_of(&drawn[0]).user[0..4],
        &0.75f32.to_ne_bytes(),
        "the repainted quad draws the new block"
    );
}

#[test]
fn equal_parameters_intern_to_one_block_so_two_elements_are_one_draw() {
    let tint = TINT.register();
    tint.set_params(Tint { amount: 0.4 });
    let (first, _one) = zgui_custom::custom(Pane {
        handle: tint.clone(),
    });
    let (second, _two) = zgui_custom::custom(Pane {
        handle: tint.clone(),
    });
    let (mut app, seen) = app(SHEET, {
        let mut panes = Some((first, second));
        move |cx: &mut BuildCx<'_>| {
            let (first, second) = panes.take().expect("built once");
            Box::new(
                zgui_elements::r#box()
                    .class("root")
                    .child(first)
                    .child(second)
                    .into_view()
                    .build(cx),
            )
        }
    });
    app.settle(16);

    let drawn = seen.shader_quads();
    assert_eq!(drawn.len(), 2, "two elements are two shader quads");
    assert_eq!(
        drawn[0].field("params"),
        drawn[1].field("params"),
        "and one parameter block between them, which is what makes them one draw call"
    );
}

/// The handle is taken once, so a frame recorded at any moment draws the same effect.
#[test]
fn registering_the_same_effect_twice_names_one_handle() {
    assert_eq!(TINT.register().id(), TINT.register().id());
}
