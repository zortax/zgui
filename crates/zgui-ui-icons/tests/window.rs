//! What a real window draws for an `<Icon/>`.
//!
//! Every other assertion about this component reads the document: the outline is on the element,
//! the square beside it, the class list right. All of them are satisfied by an icon that renders as
//! empty space, which is exactly the state a component gallery's icon card was found in — the
//! properties were written, the box was built, and no stage turned either into a primitive.
//!
//! So this drives the component the way an application does: through the umbrella, over the
//! headless platform, and asks the compositor what it drew. It is deliberately the *component*
//! rather than the `<vector>` element under it, because the size of an icon comes from the sheet the
//! component installs — `--zui-icon-size` resolving through a second variable to a fallback — and a
//! box that resolved to nothing paints nothing however well the element works.

mod screen;

use std::sync::Mutex;

use zgui::platform::{AppHandler, PlatformError};
use zgui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::CHECK;

/// One shape a frame was handed.
#[derive(Clone, Copy, Debug)]
struct Shape {
    /// How wide it is, in device pixels.
    width: f64,
    /// How tall it is.
    height: f64,
    /// What fills it, as premultiplied sRGB, when a solid colour does.
    fill: Option<[f32; 4]>,
}

/// The shapes of every frame, in the order they were drawn.
static SHAPES: Mutex<Vec<Vec<Shape>>> = Mutex::new(Vec::new());

/// The shapes `drawing` drew, with the solid colour each is filled with.
fn shapes(drawing: &zgui_testkit_compositor::Drawing) -> Vec<Shape> {
    screen::shapes(drawing)
        .into_iter()
        .map(|(instance, [width, height])| Shape {
            width,
            height,
            fill: instance.color("fill").or_else(|| instance.color("paint")),
        })
        .collect()
}

/// Drives the application over buffers for a few turns and stops.
fn buffers(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(4);
    harness.shut_down();
    Ok(())
}

/// A row with one icon in it, coloured by the text around it and sized by the component's sheet.
#[component]
fn Card() -> impl IntoView {
    view! {
        row(class = "card") {
            Icon(icon = CHECK, size = IconSize::Lg)
        }
    }
}

/// The sheet an application writes. It says nothing about the icon's size — that is the point.
const SHEET: &str = ":root { color: rgb(0, 128, 255) }
                     .card { display: flex; padding: 8px }";

/// The gallery's blank card, stated as one assertion.
#[test]
fn an_icon_mounted_in_a_window_paints_a_shape_the_size_its_own_sheet_gave_it() {
    app()
        .with_compositor_client(Box::new(zgui_testkit_compositor::driver))
        .with_title("icon")
        .with_size(200.0, 120.0)
        .with_stylesheet(SHEET)
        .with_compositor_renderer(screen::watched(|| {
            Box::new(|drawing| {
                SHAPES
                    .lock()
                    .expect("the log is not poisoned")
                    .push(shapes(drawing));
            })
        }))
        .run_on(buffers, || view! { Card() })
        .expect("the application ran");

    let frames = SHAPES.lock().expect("the log is not poisoned").clone();
    assert!(!frames.is_empty(), "no frame was drawn at all");
    let settled = frames.last().expect("a frame").clone();
    assert_eq!(
        settled.len(),
        1,
        "an icon mounted in a window put no shape in the last frame's display list: {frames:?}"
    );

    // Twenty pixels is `--zui-icon-lg`'s fallback, reached through `--zui-icon-size` with no token
    // sheet installed, and the outline is fitted uniformly into a box of that side. So what the
    // frame drew has to be the constant's own extent at exactly that ratio — which is a claim about
    // the sheet, about the fit and about the outline at once, and which "larger than nothing" would
    // not be: a box that collapsed to a point still produces a primitive, of no extent.
    //
    // A path mask is the geometry rounded out to whole pixels, which adds less than one pixel at
    // each edge. That is less than the step to the next icon size.
    let written = zgui::elements::kurbo::Shape::bounding_box(&CHECK.path());
    let ratio = 20.0 / CHECK.view_box();
    let shape = settled[0];
    let rounding = 2.0;
    let fits = |drawn: f64, asked: f64| drawn > asked - 0.01 && drawn < asked + rounding + 0.01;
    assert!(
        fits(shape.width, written.width() * ratio) && fits(shape.height, written.height() * ratio),
        "the icon was drawn {} by {}, not the {} by {} its own sheet and its own outline ask for",
        shape.width,
        shape.height,
        written.width() * ratio,
        written.height() * ratio
    );
    assert!(
        shape.width > 1.0 && shape.height > 1.0,
        "the icon was drawn at no size at all, so the comparison above compared two zeroes"
    );
    let fill = shape
        .fill
        .expect("the shape was filled with a solid colour");
    assert!(
        fill[0] < 0.01 && (fill[1] - 128.0 / 255.0).abs() < 0.01 && fill[2] > 0.99,
        "an icon with no paint of its own takes the colour of the text around it, not {fill:?}"
    );
}
