//! An effect painted by a custom element, through the real frame loop.
//!
//! The device tests in `zgui-render-wgpu` say what an effect draws. This says what reaches the
//! display list on the way there: one instance, positioned by CSS, carrying the effect and the
//! parameters the handle was set to. It runs with a renderer that records rather than draws, so it
//! needs no device.

// The root a `shader!` expansion names its crates through. An application reaches it as
// `zgui::shader`; a crate with no umbrella over it supplies the root itself.
extern crate zgui_shader as zgui;

use std::sync::Arc;

use zgui_custom::{CustomElement, CustomHandle, CustomLayoutCx, CustomMeasured, ScenePainter};
use zgui_geom::{DevicePx, Point, Rect, Size};
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_runtime::{App, AppError, Runtime};
use zgui_shader::{ShaderEffect, ShaderHandle, ShaderParams, ShaderPainterExt, shader};
use zgui_view::{Anchor, BuildCx, IntoView, View};

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

/// An application whose window records rather than draws.
fn app<V>(css: &str, view: V) -> Harness<Runtime>
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    let handler = App::new()
        .with_title("shader")
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
        .with_custom(Box::new(|_document| zgui_custom::sources()))
        .into_handler(view)
        .expect("the reactive runtime installs");
    Harness::new(handler)
}

/// The fixture, and the two handles that drive it.
fn paned() -> (Harness<Runtime>, ShaderHandle<Tint>, CustomHandle<Pane>) {
    let tint = TINT.register();
    let (view, element) = zgui_custom::custom(Pane {
        handle: tint.clone(),
    });
    let app = app(SHEET, {
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
    (app, tint, element)
}

/// Every shaded rectangle the window last composed.
fn shaded(app: &mut Harness<Runtime>) -> Vec<zgui_scene::ShadedQuad> {
    app.app_mut().windows_mut()[0]
        .scene()
        .primitives
        .shaded
        .clone()
}

const SHEET: &str = "root { display: block; width: 400px; height: 300px; padding: 10px }";

#[test]
fn an_effect_reaches_the_display_list_as_one_instance_where_css_put_it() {
    let (mut app, tint, _element) = paned();
    tint.set_params(Tint { amount: 0.5 });
    app.settle(16);

    let drawn = shaded(&mut app);
    assert_eq!(drawn.len(), 1, "one effect is one instance");
    let quad = &drawn[0];
    assert_eq!(
        (quad.bounds[0], quad.bounds[1]),
        (10.0, 10.0),
        "the effect sits at the content box the sheet's padding produced"
    );
    assert_eq!(
        (quad.bounds[2], quad.bounds[3]),
        (380.0, 40.0),
        "at the size layout gave the element"
    );
    assert!(quad.shader_id().is_some(), "it names the registered effect");
    assert_eq!(
        quad.radii[0], 4.0,
        "and carries the corner it was drawn with"
    );
}

#[test]
fn the_parameters_the_handle_was_set_to_are_the_ones_the_instance_names() {
    let (mut app, tint, element) = paned();
    tint.set_params(Tint { amount: 0.25 });
    app.settle(16);
    let first = shaded(&mut app)[0].params_slot();

    tint.set_params(Tint { amount: 0.75 });
    // The parameters changed, so the element owes a repaint: an effect that is replayed draws the
    // parameters it was painted with, which is the whole reason this is said out loud.
    element.repaint();
    app.settle(16);
    let second = shaded(&mut app)[0].params_slot();

    assert_ne!(
        first, second,
        "two different parameter blocks are two different slots"
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
    let mut app = app(SHEET, {
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

    let drawn = shaded(&mut app);
    assert_eq!(drawn.len(), 2, "two elements are two instances");
    assert_eq!(
        drawn[0].params_slot(),
        drawn[1].params_slot(),
        "and one parameter block between them, which is what makes them one draw call"
    );
}

/// The handle is taken once, so a display list built at any moment names the same effect.
#[test]
fn registering_the_same_effect_twice_names_one_handle() {
    assert_eq!(TINT.register().id(), TINT.register().id());
}
