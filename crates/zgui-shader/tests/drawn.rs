//! Effects drawn on a graphics device through the compositor's reference renderer.
//!
//! The other tests read what the compositor was asked to draw. These read the pixels, which is
//! what says that the program `shader!` compiled is one a renderer builds and runs with the
//! parameters a style sheet wrote. A machine with no graphics device asserts nothing here.

// The root a `shader!` expansion names its crates through.
extern crate zgui_shader as zgui;

use std::sync::Arc;

use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_runtime::{App, AppError, Runtime};
use zgui_shader::{NoParams, ShaderEffect, ShaderParams, shader};
use zgui_testkit_compositor::{Composed, Device};
use zgui_view::{BuildCx, IntoView, View};

/// What the paint effect draws with.
#[repr(C)]
#[derive(Clone, Copy, Default, ShaderParams)]
struct Level {
    /// The green channel the effect shades.
    level: f32,
}

/// A paint effect that shades one colour, whose green comes from the style sheet.
static GREEN: ShaderEffect<Level> = shader! {
    name: "drawn-green",
    mode: Paint,
    params: Level,
    source: r#"
        struct Params {
            level: f32,
        }

        fn shade(in: ShaderInput, params: Params) -> vec4<f32> {
            return vec4<f32>(0.0, params.level, 0.0, 1.0);
        }
    "#,
};

/// A filter effect that swaps the red and blue channels of what it reads.
static SWAP: ShaderEffect<NoParams> = shader! {
    name: "drawn-swap",
    mode: Filter,
    source: r#"
        fn apply(
            in: ShaderInput,
            params: Params,
            source: texture_2d<f32>,
            source_sampler: sampler,
            region: FilterSource,
        ) -> vec4<f32> {
            return source_at(source, source_sampler, region, in.local).bgra;
        }
    "#,
};

/// An application whose window draws one card onto a device, and the card's composed pixels.
///
/// Returns `None` when no device opens.
fn app(css: &str) -> Option<(Harness<Runtime>, Composed)> {
    if let Err(failure) = Device::open([200, 200]) {
        eprintln!("skipped: no usable graphics device ({failure})");
        return None;
    }
    let composed = Composed::default();
    let into = composed.clone();
    let handler = App::new()
        .with_title("drawn")
        .with_size(200.0, 200.0)
        .with_stylesheet(css)
        .with_compositor(zgui_testkit_compositor::driver(
            zgui_testkit_compositor::fake_text_compositor(),
        ))
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: compositor_types::RenderTarget| {
                let into = into.clone();
                Ok::<_, AppError>(zgui_testkit_compositor::factory(move || {
                    let mut device = Device::open(target.size)
                        .expect("a device opens, which the test answered beforehand")
                        .composing_into(into);
                    compositor_api::Renderer::configure(&mut device, target);
                    device
                }))
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(zgui_testkit::MonoShaper::new()))
        }))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            Box::new(zgui_elements::r#box().class("card").into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    Some((Harness::new(handler), composed))
}

/// Returns the composed pixel at the card's centre.
fn centre(composed: &Composed) -> [u8; 4] {
    composed
        .pixels()
        .expect("the device composed a frame")
        .rgba(50, 50)
}

#[test]
fn a_paint_effect_shades_its_box_with_the_parameters_the_sheet_wrote() {
    let _ = GREEN.register();
    let Some((mut app, composed)) = app("root { display: block; background-color: #000000 } \
         box { display: block; width: 100px; height: 100px; \
         --zgui-shader: drawn-green; --drawn-green-level: 1 }")
    else {
        return;
    };
    app.settle(16);

    assert_eq!(centre(&composed), [0, 255, 0, 255]);
}

#[test]
fn a_filter_effect_runs_over_the_content_of_its_box() {
    let _ = SWAP.register();
    let Some((mut app, composed)) = app("root { display: block; background-color: #000000 } \
         box { display: block; width: 100px; height: 100px; background-color: #ff0000; \
         --zgui-filter: drawn-swap }")
    else {
        return;
    };
    app.settle(16);

    assert_eq!(
        centre(&composed),
        [0, 0, 255, 255],
        "the red box reads blue through the effect"
    );
}
