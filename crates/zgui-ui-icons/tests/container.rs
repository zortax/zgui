//! An icon sized from its container, read off the frame a window draws.
//!
//! The container hook is `--zui-icon-md` (and its siblings). Every icon declares
//! `--zui-icon-size` again from those steps, so the steps are what a container sets.

use std::sync::Mutex;

use zgui::platform::{AppHandler, PlatformError};
use zgui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::CHECK;

/// The sizes of the shapes in the last frame, as `[width, height]`.
static SIZES: Mutex<Vec<[f64; 2]>> = Mutex::new(Vec::new());

/// A probe that keeps the sizes of the last frame's shapes.
struct Recorder;

impl zgui::runtime::FrameProbe for Recorder {
    fn frame_ended(&self, window: &zgui::runtime::Window) {
        let scene = window.scene();
        let primitives = &scene.primitives;
        let bounds = |b: [f32; 4]| [f64::from(b[2]), f64::from(b[3])];
        let mut frame: Vec<[f64; 2]> = primitives
            .vectors
            .iter()
            .map(|item| {
                let placed = item.placed_bounds();
                [placed.width(), placed.height()]
            })
            .collect();
        frame.extend(primitives.marks.iter().map(|mark| bounds(mark.bounds)));
        frame.extend(
            primitives
                .mono_sprites
                .iter()
                .map(|sprite| bounds(sprite.bounds)),
        );
        *SIZES.lock().expect("the log is not poisoned") = frame;
    }
}

/// Drives the application over buffers for a few turns and stops.
fn buffers(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(4);
    harness.shut_down();
    Ok(())
}

#[test]
fn a_container_sets_the_size_of_the_icons_under_it_through_the_step() {
    app()
        .with_title("icon")
        .with_size(200.0, 120.0)
        .with_stylesheet(".card { display: flex; padding: 8px }")
        .with_probe(std::rc::Rc::new(Recorder))
        .with_compositor_renderer(Box::new(|_size| {
            Box::new(zgui_testkit_compositor::NullRenderer::default())
        }))
        .run_on(buffers, || {
            view! { row(class = "card", var:--zui-icon-md = "32px") { Icon(icon = CHECK) } }
        })
        .expect("the application ran");

    let sizes = SIZES.lock().expect("the log is not poisoned").clone();
    assert_eq!(sizes.len(), 1, "one icon, one shape: {sizes:?}");

    // The outline is fitted into the icon's box at the ratio of the box to its view box. A coverage
    // sprite rounds out to whole pixels, so up to two pixels more are allowed.
    let written = zgui::elements::kurbo::Shape::bounding_box(&CHECK.path());
    let ratio = 32.0 / CHECK.view_box();
    let [width, height] = sizes[0];
    let fits = |drawn: f64, asked: f64| drawn > asked - 0.01 && drawn < asked + 2.01;
    assert!(
        fits(width, written.width() * ratio) && fits(height, written.height() * ratio),
        "the icon was drawn {width} by {height}, and a 32px box asks for {} by {}",
        written.width() * ratio,
        written.height() * ratio
    );
}
