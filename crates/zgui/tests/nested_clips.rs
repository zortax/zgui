//! A rounded `overflow: hidden` box inside two rounded clipping ancestors, from a style sheet to
//! the clip chain the window draws through.

use zgui::platform::{AppHandler, PlatformError, SurfaceEvent};
use zgui::prelude::*;
use zgui_platform_headless::Harness;
use zgui_testkit_compositor::{Drawing, Screen};

const SHEET: &str = zgui::css!(
    "root { display: block; width: 300px; height: 200px }
     .r { display: block; overflow: hidden; border-radius: 12px; padding: 16px }
     .clip { display: block; overflow: hidden; border-radius: 24px; width: 200px; height: 120px }
     .fill { display: block; width: 100%; height: 100%; background-color: #ff0000 }"
);

/// Returns what the application shows, with a rounded clip inside two rounded clipping ancestors.
fn drawn() -> Drawing {
    let screen = Screen::new();
    let shown = screen.shown();
    let mut screen = Some(screen);
    let drive = |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
        let mut harness = Harness::new(handler);
        harness.deliver_to_first(SurfaceEvent::Resized(zgui::geom::Size::new(
            zgui::geom::DevicePx(300.0),
            zgui::geom::DevicePx(200.0),
        )));
        harness.settle(32);
        Ok(())
    };
    zgui::app()
        .with_title("nested-clips")
        .with_size(300.0, 200.0)
        .with_stylesheet(SHEET)
        .with_compositor_renderer(Box::new(move |_size| {
            Box::new(screen.take().expect("the application opens one window"))
                as Box<dyn compositor_api::Renderer>
        }))
        .run_on(drive, || {
            view! {
                box(class = "root") {
                    box(class = "r") { box(class = "r") { box(class = "clip") {
                        box(class = "fill") {}
                    } } }
                }
            }
        })
        .expect("the application ran");
    shown.drawing().expect("the window drew a frame")
}

#[test]
fn a_third_rounded_clip_keeps_its_corners() {
    let drawing = drawn();

    let fill = drawing
        .quads()
        .into_iter()
        .find(|quad| quad.color("fill") == Some([1.0, 0.0, 0.0, 1.0]))
        .unwrap_or_else(|| panic!("the fill is drawn:\n{drawing}"));
    let clip = fill.field("clip").unwrap_or_else(|| {
        panic!(
            "the fill is drawn through the clips around it: {}",
            fill.text()
        )
    });
    assert!(
        clip.matches("radii=").count() >= 3,
        "the fixture has to nest three rounded clips: {clip}"
    );
    let innermost = clip.rsplit(" > ").next().unwrap_or(clip);
    assert!(
        innermost.contains("radii=[24, 24, 24, 24, 24, 24, 24, 24]"),
        "the innermost clip's corners are applied: {clip}"
    );
}
