//! A window drawn offscreen, read back from the image its last frame presented.

use zgui::app::Presented;
use zgui::platform::{AppHandler, PlatformError, SurfaceEvent};
use zgui::prelude::*;
use zgui_platform_headless::Harness;
use zgui_testkit_compositor::Device;

/// The window's extent, in device pixels at scale one.
const WIDTH: i32 = 64;
/// The window's extent, in device pixels at scale one.
const HEIGHT: i32 = 48;

const SHEET: &str = zgui::css!(
    ".root { width: 64px; height: 48px; background-color: #3366cc }
     .corner { width: 16px; height: 16px; background-color: #ff0000 }"
);

/// Returns `true` where this machine has a device the test can draw on.
fn available() -> bool {
    match Device::open([8, 8]) {
        Ok(_) => true,
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            false
        }
    }
}

#[test]
fn an_offscreen_window_keeps_the_image_its_last_frame_presented() {
    if !available() {
        return;
    }
    let presented = Presented::default();
    let drive = |handler: Box<dyn AppHandler>| -> Result<(), PlatformError> {
        let mut harness = Harness::new(handler);
        harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
            scale_factor: 1.0,
            size: zgui::geom::Size::new(
                zgui::geom::DevicePx(WIDTH as f32),
                zgui::geom::DevicePx(HEIGHT as f32),
            ),
        });
        harness.settle(64);
        Ok(())
    };
    zgui::app()
        .with_title("offscreen")
        .with_size(WIDTH as f32, HEIGHT as f32)
        .with_stylesheet(SHEET)
        .with_offscreen(presented.clone())
        .run_on(drive, || {
            view! {
                box(class = "root") {
                    box(class = "corner") {}
                }
            }
        })
        .expect("the application ran");

    let pixels = presented.take().expect("the window presented a frame");
    assert_eq!((pixels.size().width, pixels.size().height), (WIDTH, HEIGHT));
    assert_eq!(pixels.rgba(4, 4), [255, 0, 0, 255], "the corner box");
    assert_eq!(pixels.rgba(40, 30), [0x33, 0x66, 0xcc, 255], "the root");
    assert!(presented.take().is_none(), "a take empties the handle");
}
