//! `corner-shape` through the whole application, from a style sheet to what the window draws.
//!
//! The unit tests say the property parses and the device tests say each shape draws. What is
//! asserted here is the wiring in between: that a style sheet reaches the box's own quad *and* the
//! clip the box gives its children, which are the two halves that have to agree or content shows
//! its corners past the card holding it.

use zgui::platform::{AppHandler, PlatformError, SurfaceEvent};
use zgui::prelude::*;
use zgui_platform_headless::Harness;
use zgui_testkit_compositor::{Drawing, Instance, Screen};

const SHEET: &str = zgui::css!(
    "root { display: block; width: 300px; height: 200px }
     .card {
        display: block;
        width: 120px;
        height: 80px;
        border-radius: 24px;
        background-color: #ff0000;
        overflow: hidden;
        --zgui-corner-shape: squircle;
     }
     .inner { display: block; width: 120px; height: 80px; background-color: #00ff00 }
     .plain {
        display: block;
        width: 60px;
        height: 40px;
        border-radius: 12px;
        background-color: #0000ff;
     }"
);

/// The corner exponents a quad or a clip link cut to a squircle carries.
const SQUIRCLE: &str = "shapes=[4, 4, 4, 4]";

/// Returns what the application shows, with one smoothed card holding one child.
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
        .with_title("corner-shape")
        .with_size(300.0, 200.0)
        .with_stylesheet(SHEET)
        .with_compositor_client(Box::new(zgui_testkit_compositor::driver))
        .with_compositor_renderer(Box::new(move |_size| {
            Box::new(screen.take().expect("the application opens one window"))
                as Box<dyn iceblink::Renderer>
        }))
        .run_on(drive, || {
            view! {
                box(class = "root") {
                    box(class = "card") { box(class = "inner") {} }
                    box(class = "plain") {}
                }
            }
        })
        .expect("the application ran");
    shown.drawing().expect("the window drew a frame")
}

/// Returns the quad filled with the opaque colour `rgb`.
fn filled(drawing: &Drawing, rgb: [f32; 3]) -> &Instance {
    let [r, g, b] = rgb;
    drawing
        .quads()
        .into_iter()
        .find(|quad| quad.color("fill") == Some([r, g, b, 1.0]))
        .unwrap_or_else(|| panic!("no quad is filled with {rgb:?}:\n{drawing}"))
}

#[test]
fn a_style_sheet_cuts_the_box_it_names_and_the_clip_that_box_gives_its_children() {
    let drawing = drawn();

    let card = filled(&drawing, [1.0, 0.0, 0.0]);
    assert!(
        card.has(SQUIRCLE),
        "the box the sheet named is cut to the shape it named: {}",
        card.text()
    );

    // The child is drawn through the clip the card gives it, and that clip has to be cut the same
    // way or the child's own corners show past the card's.
    let inner = filled(&drawing, [0.0, 1.0, 0.0]);
    let clip = inner.field("clip").unwrap_or_else(|| {
        panic!(
            "the child is drawn through a clip of its own: {}",
            inner.text()
        )
    });
    assert!(
        clip.contains("radii=") && clip.contains(SQUIRCLE),
        "the clip the card gives its children is cut the way the card is: {clip}"
    );
}

/// A box outside the subtree that named a shape keeps the ellipse, or every rounded box in every
/// document changes shape the moment one card asks for a smoothed corner.
#[test]
fn a_box_beside_the_one_that_named_a_shape_is_still_the_ellipse() {
    let drawing = drawn();
    let plain = filled(&drawing, [0.0, 0.0, 1.0]);
    assert!(
        plain.field("radii").is_some() && plain.field("shapes").is_none(),
        "the sibling is a round box: {}",
        plain.text()
    );
}

/// The shape inherits, because every unregistered custom property does — and here that is wanted
/// rather than tolerated: "this subtree uses smoothed corners" is a thing a style sheet should be
/// able to say once. A descendant that wants the ellipse back writes `round`.
#[test]
fn the_shape_inherits_and_a_descendant_can_take_it_back() {
    let drawing = drawn();
    // The child inside the card inherited the shape. It has no radii, so it draws exactly as it
    // would have — which is why the inheritance is survivable as well as wanted.
    let inner = filled(&drawing, [0.0, 1.0, 0.0]);
    assert!(
        inner.field("radii").is_none() && inner.has(SQUIRCLE),
        "the child carries the inherited shape: {}",
        inner.text()
    );
}
