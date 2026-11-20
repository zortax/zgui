//! What a real window draws for an `<Icon/>`.
//!
//! Every other assertion about this component reads the document: the outline is on the element,
//! the square beside it, the class list right. All of them are satisfied by an icon that renders as
//! empty space, which is exactly the state a component gallery's icon card was found in — the
//! properties were written, the box was built, and no stage turned either into a primitive.
//!
//! So this drives the component the way an application does: through the umbrella, over the
//! headless platform, and asks the renderer what it was handed. It is deliberately the *component*
//! rather than the `<vector>` element under it, because the size of an icon comes from the sheet the
//! component installs — `--zui-icon-size` resolving through a second variable to a fallback — and a
//! box that resolved to nothing paints nothing however well the element works.

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
    /// Whether the size is the geometry's own. A vector item records its geometry. A coverage
    /// sprite records its tile, which is the geometry rounded out to whole pixels.
    exact: bool,
}

/// The colour `paint` resolves to in `scene`, as premultiplied sRGB, when it is a solid colour.
fn solid(scene: &zgui::scene::Scene, paint: zgui::scene::PaintRef) -> Option<[f32; 4]> {
    match scene.paints.get(paint.id()?) {
        Some(zgui::scene::Paint::Solid(color)) => Some(color.to_premultiplied_srgb()),
        _ => None,
    }
}

/// A shape of `[x, y, width, height]`.
fn sized(bounds: [f32; 4], fill: Option<[f32; 4]>) -> Shape {
    Shape {
        width: f64::from(bounds[2]),
        height: f64::from(bounds[3]),
        fill,
        exact: false,
    }
}

/// The shapes of every frame of the run, in order.
static SHAPES: Mutex<Vec<Vec<Shape>>> = Mutex::new(Vec::new());

/// A probe that records the shapes of every frame.
///
/// A shape arrives as a vector item, a mark item, a coverage sprite or a quad, depending on the
/// route the display list gives it, so all four are recorded.
struct Recorder;

impl zgui::runtime::FrameProbe for Recorder {
    fn frame_ended(&self, window: &zgui::runtime::Window) {
        let scene = window.scene();
        let primitives = &scene.primitives;
        let mut frame: Vec<Shape> = primitives
            .vectors
            .iter()
            .map(|item| {
                let bounds = item.placed_bounds();
                Shape {
                    width: bounds.width(),
                    height: bounds.height(),
                    fill: item.fill.and_then(|reference| solid(scene, reference)),
                    exact: true,
                }
            })
            .collect();
        frame.extend(
            primitives
                .marks
                .iter()
                .map(|mark| sized(mark.bounds, solid(scene, mark.paint))),
        );
        frame.extend(
            primitives
                .mono_sprites
                .iter()
                .map(|sprite| sized(sprite.bounds, Some(sprite.color))),
        );
        frame.extend(
            primitives
                .quads
                .iter()
                .map(|quad| sized(quad.bounds, solid(scene, quad.fill))),
        );
        SHAPES.lock().expect("the log is not poisoned").push(frame);
    }
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
        .with_title("icon")
        .with_size(200.0, 120.0)
        .with_stylesheet(SHEET)
        .with_probe(std::rc::Rc::new(Recorder))
        .with_compositor_renderer(Box::new(|_size| {
            Box::new(zgui_testkit_compositor::NullRenderer::default())
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
    // A coverage sprite is the geometry rounded out to whole pixels, which adds less than one pixel
    // at each edge. That is less than the step to the next icon size.
    let written = zgui::elements::kurbo::Shape::bounding_box(&CHECK.path());
    let ratio = 20.0 / CHECK.view_box();
    let shape = settled[0];
    let rounding = if shape.exact { 0.0 } else { 2.0 };
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
