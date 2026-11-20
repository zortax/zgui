//! A window drawing through a presenter in buffer-age mode.
//!
//! The image a frame is composed into is acquired before the frame is built, and its age decides
//! what the frame redraws: everything that changed since the image was last presented. A window
//! whose image could not be acquired builds no frame and keeps what it owes.

mod support;

use std::sync::Arc;

use compositor_api::Renderer;
use compositor_types::RenderTarget;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_runtime::{App, AppError, Runtime};
use zgui_testkit_compositor::{Aged, Drawn, Frames, Refusals};
use zgui_view::{BuildCx, ClassName, IntoView, View};

/// Three swatches far apart, each of which can be lit.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .swatch { display: block; width: 40px; height: 20px; margin-bottom: 60px;
                             background-color: #303030 }
                   .lit { background-color: #c0c0c0 }";

/// The swatches' rectangles on the device, top first.
const SWATCHES: [Rect<i32, Device>; 3] = [
    Rect::new(Point::new(0, 0), Size::new(40, 20)),
    Rect::new(Point::new(0, 80), Size::new(40, 20)),
    Rect::new(Point::new(0, 160), Size::new(40, 20)),
];

/// A window of three swatches lit by `lit`, drawn into a rotation of `images` images.
fn window(lit: [RwSignal<bool>; 3], images: usize) -> (Harness<Runtime>, Frames, Refusals) {
    let (frames, refusals) = (Frames::default(), Refusals::default());
    let (into, refused) = (frames.clone(), refusals.clone());
    let handler = App::new()
        .with_title("buffer age")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_compositor(support::compositor())
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Aged::new(images, into.clone(), refused.clone());
                renderer.configure(target);
                Ok::<Box<dyn Renderer>, AppError>(Box::new(renderer))
            },
        ))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            let mut root = zgui_elements::column().class("root");
            for on in lit {
                root = root.child(
                    zgui_elements::column()
                        .class("swatch")
                        .class_toggle(ClassName::new("lit"), move || on.get()),
                );
            }
            Box::new(root.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(8);
    (harness, frames, refusals)
}

/// Whether the draw damage of `frame` covers every pixel of `rect`.
fn redraws(frame: &Drawn, rect: Rect<i32, Device>) -> bool {
    let draw = &frame.damage.draw;
    draw.is_full()
        || draw.rects().iter().any(|held| {
            held.origin.x <= rect.origin.x
                && held.origin.y <= rect.origin.y
                && held.origin.x + held.size.width >= rect.origin.x + rect.size.width
                && held.origin.y + held.size.height >= rect.origin.y + rect.size.height
        })
}

#[test]
fn a_frame_redraws_what_changed_since_its_image_was_last_presented() {
    let lit = [
        RwSignal::new(false),
        RwSignal::new(false),
        RwSignal::new(false),
    ];
    let (mut harness, frames, _) = window(lit, 3);
    // Three frames, so every image has been presented and has a known age.
    for on in lit {
        on.set(true);
        harness.settle(8);
    }
    frames.clear();

    // Each swatch turns off in a frame of its own. The third frame is composed into the image
    // presented three frames before, so it redraws all three swatches.
    for on in lit {
        on.set(false);
        harness.settle(8);
    }
    let drawn = frames.drawn();
    assert_eq!(drawn.len(), 3, "three changes drew {} frames", drawn.len());
    let last = &drawn[2];
    assert!(
        !last.damage.draw.is_full(),
        "the frame redrew the whole output, so its image's age was not used"
    );
    for (index, swatch) in SWATCHES.iter().enumerate() {
        assert!(
            redraws(last, *swatch),
            "swatch {index} changed since the image was presented, and the frame drew {:?}",
            last.damage.draw.rects()
        );
    }
}

#[test]
fn a_frame_whose_image_is_not_acquired_is_not_built_and_keeps_its_damage() {
    let lit = [
        RwSignal::new(false),
        RwSignal::new(false),
        RwSignal::new(false),
    ];
    let (mut harness, frames, refusals) = window(lit, 2);
    frames.clear();

    refusals.set(1);
    lit[0].set(true);
    harness.pump();
    assert_eq!(refusals.left(), 0, "the frame never asked for an image");
    assert!(
        frames.drawn().is_empty(),
        "a frame was drawn without an image to draw into"
    );

    // The frame that could not draw asks for another one refresh later.
    for _ in 0..4 {
        harness.advance(std::time::Duration::from_millis(20));
        harness.pump();
    }
    let drawn = frames.drawn();
    assert!(
        drawn.iter().any(|frame| redraws(frame, SWATCHES[0])),
        "the change the unacquired frame owed was never drawn: {:?}",
        drawn
            .iter()
            .map(|frame| frame.damage.draw.rects().to_vec())
            .collect::<Vec<_>>()
    );
}
