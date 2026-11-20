//! A variable face drawn through the compositor at the instance each run was shaped at.

use std::sync::Arc;

use zgui::platform::{AppHandler, PlatformError, SurfaceEvent};
use zgui::prelude::*;
use zgui_platform_headless::Harness;
use zgui_testkit_compositor::{Drawing, Screen};

const SHEET: &str = zgui::css!(
    ".root { display: block; width: 300px; height: 200px }
     .light, .heavy {
        position: absolute;
        left: 10px;
        font-family: Rubik;
        font-size: 48px;
        color: #000000;
     }
     .light { top: 10px; font-weight: 400 }
     .heavy { top: 100px; font-weight: 800 }"
);

/// Draws one letter of the shipped variable face at 400 and one at 800, and returns the last
/// frame.
fn drawn() -> Drawing {
    let fonts = Fonts::shipped_only();
    let path = format!(
        "{}/../zgui-text-parley/tests/fonts/RubikSubset-Variable.ttf",
        env!("CARGO_MANIFEST_DIR")
    );
    let data = std::fs::read(&path).unwrap_or_else(|error| panic!("reading {path}: {error}"));
    fonts
        .register(Arc::new(data), None)
        .expect("the variable face registers");

    let screen = Screen::new();
    let shown = screen.shown();
    let mut screen = Some(screen);
    let drive = |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
        let mut harness = Harness::new(handler);
        harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
            scale_factor: 1.0,
            size: zgui::geom::Size::new(zgui::geom::DevicePx(300.0), zgui::geom::DevicePx(200.0)),
        });
        harness.settle(64);
        Ok(())
    };
    zgui::app()
        .with_title("variations")
        .with_size(300.0, 200.0)
        .with_fonts(fonts)
        .with_stylesheet(SHEET)
        .with_compositor_renderer(Box::new(move |_size| {
            Box::new(screen.take().unwrap_or_default()) as Box<dyn compositor_api::Renderer>
        }))
        .run_on(drive, || {
            view! {
                box(class = "root") {
                    box(class = "light") { text { "O" } }
                    box(class = "heavy") { text { "O" } }
                }
            }
        })
        .expect("the application ran");
    shown.drawing().expect("the window drew a frame")
}

/// The same letter at the same size and phase is wider at 800 than at 400, which it is only when
/// each run is rasterised at its own coordinates on the `wght` axis.
#[test]
fn a_variable_face_draws_each_run_at_the_weight_it_was_shaped_at() {
    let drawing = drawn();
    let mut glyphs: Vec<[f32; 4]> = drawing
        .glyphs()
        .iter()
        .map(|glyph| glyph.bounds())
        .collect();
    glyphs.sort_by(|a, b| a[1].total_cmp(&b[1]));
    let [light, heavy] = glyphs[..] else {
        panic!("two letters were drawn: {drawing:?}");
    };
    assert_eq!(light[0], heavy[0], "both letters start at one pixel");
    assert!(
        heavy[2] >= light[2] + 2.0,
        "the letter at 800 is no wider than the one at 400: {heavy:?} against {light:?}"
    );
}
