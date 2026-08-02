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

use std::sync::Arc;
use std::sync::Mutex;

use zgui::platform::{AppHandler, PlatformError, Surface};
use zgui::prelude::*;
use zgui::runtime::AppError;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::CHECK;

/// One shape a frame was handed.
#[derive(Clone, Copy, Debug)]
struct Shape {
    /// How wide the geometry is, in device pixels.
    width: f64,
    /// How tall it is.
    height: f64,
    /// What fills it, as premultiplied sRGB, when a solid colour does.
    fill: Option<[f32; 4]>,
}

/// The shapes of every frame of the run, in order.
static SHAPES: Mutex<Vec<Vec<Shape>>> = Mutex::new(Vec::new());

/// A renderer that records the vector content of every frame and draws nothing.
#[derive(Debug, Default)]
struct Recorder {
    /// Where tiles go, so the upload path still runs.
    atlas: zgui::atlas::MemorySink,
    /// What it was pointed at.
    target: Option<zgui::render::RenderTarget>,
}

impl zgui::render::Renderer for Recorder {
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        zgui::render::RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: zgui::render::RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<zgui::render::RenderTarget> {
        self.target
    }

    fn register_external(
        &mut self,
        _texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        zgui::render::TextureHandle(0)
    }

    fn release_external(&mut self, _handle: zgui::render::TextureHandle) {}

    fn memory(&self) -> zgui::render::MemoryReport {
        zgui::render::MemoryReport::default()
    }

    fn draw(
        &mut self,
        scene: &zgui::scene::Scene,
        _damage: &zgui::bits::DamageSet,
    ) -> zgui::render::FrameOutcome {
        use zgui::elements::kurbo::Shape as _;
        let frame = scene
            .primitives
            .vectors
            .iter()
            .map(|item| {
                let bounds = item.path.bounding_box();
                Shape {
                    width: bounds.width(),
                    height: bounds.height(),
                    fill: item
                        .fill
                        .and_then(|reference| reference.id())
                        .and_then(|id| match scene.paints.get(id) {
                            Some(zgui::scene::Paint::Solid(color)) => {
                                Some(color.to_premultiplied_srgb())
                            }
                            _ => None,
                        }),
                }
            })
            .collect();
        SHAPES.lock().expect("the log is not poisoned").push(frame);
        zgui::render::FrameOutcome::Presented(zgui::render::FrameStats::default())
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        &mut self.atlas
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
        .with_renderer(Box::new(|_surface: &Arc<dyn Surface>, target| {
            let mut renderer = Recorder::default();
            zgui::render::Renderer::configure(&mut renderer, target);
            Ok::<_, AppError>(Box::new(renderer) as Box<dyn zgui::render::Renderer>)
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
    let written = zgui::elements::kurbo::Shape::bounding_box(&CHECK.path());
    let ratio = 20.0 / CHECK.view_box();
    let shape = settled[0];
    assert!(
        (shape.width - written.width() * ratio).abs() < 0.01
            && (shape.height - written.height() * ratio).abs() < 0.01,
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
