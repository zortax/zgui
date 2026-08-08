//! The shipped gallery, drawn by a real graphics device, measured in pixels.
//!
//! # The one thing this asks
//!
//! Every drawing in this library reaches the display list, and every assertion that stops there is
//! satisfied by a rasteriser that writes nothing. A component gallery was found in exactly that
//! state: eleven icons with boxes of precisely the right size and no ink in any of them, while
//! every test of the component was green. Room is not ink.
//!
//! So this opens `examples/gallery` — the view itself, through `#[path]`, so what is measured and
//! what is shipped cannot drift apart — over the headless platform with a real device underneath,
//! reads the composed target back, and asks whether the pixels the display list claimed changed
//! anything.
//!
//! # Why the `Icon` panel in particular
//!
//! Not every drawing in the display list is meant to be seen: a ticked checkbox carries the dash it
//! would show if it were part-way, faded out. Asserting on all of them would be asserting that the
//! library never fades anything. The `Icon` panel is the opposite case — eleven marks and three
//! sizes, every one of them opaque, in front of a flat card, with nothing over them — so *every*
//! drawing inside it must mark its own rectangle, and the panel is found in the document rather
//! than by a coordinate.

#[path = "../examples/gallery/app.rs"]
#[allow(
    dead_code,
    reason = "the gallery names the window size it ships at; this fixture takes the device's"
)]
mod app;
mod desktop;
mod device;
#[path = "../examples/gallery/section/mod.rs"]
#[allow(
    dead_code,
    unused_imports,
    reason = "the gallery's sections are one module; these assertions measure the drawn ones"
)]
mod section;
#[path = "../examples/gallery/shell.rs"]
#[allow(
    dead_code,
    reason = "the shell is one module; these assertions measure the panels it lays out"
)]
mod shell;

use std::sync::{Arc, Mutex};

use zgui::geom::{Device, DevicePx, Rect};
use zgui::platform::{AppHandler, PlatformError};
use zgui::prelude::IntoView;
use zgui::view;

use crate::app::GalleryProps;
use crate::desktop::census::Census;
use crate::desktop::grab::{self, Grab};
use crate::device::Log;
use crate::device::frame::Frame;

/// How wide the surface is drawn, in device pixels: the 3840-wide head these pictures came from.
const WIDTH: f32 = 3840.0;

/// How tall it is, which is that head less the compositor's own bar.
const HEIGHT: f32 = 2125.0;

/// The ratio it is drawn at, which is that head's.
const SCALE: f64 = 1.2;

/// The runs, or nothing on a machine with no graphics device.
macro_rules! opened {
    () => {
        match run() {
            Some(opened) => opened,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// One run of the gallery: what it drew, and where its panels ended up.
struct Opened {
    /// Every frame, in the order they were drawn.
    frames: Vec<Frame>,
    /// The rectangle of each panel that was found, by title.
    panels: Vec<(String, Rect<DevicePx, Device>)>,
}

/// Drives the application over buffers until it settles, and records where its panels are.
fn buffers(
    panels: &Mutex<Vec<(String, Rect<DevicePx, Device>)>>,
) -> impl FnOnce(Box<dyn AppHandler>) -> Result<(), PlatformError> + '_ {
    move |handler| {
        let mut harness = zgui_platform_headless::Harness::new(handler);
        // The extent and the ratio, told to the application the way a compositor tells it.
        harness.deliver_to_first(zgui::platform::SurfaceEvent::ScaleFactorChanged {
            scale_factor: SCALE,
            size: zgui::geom::Size::new(zgui::geom::DevicePx(WIDTH), zgui::geom::DevicePx(HEIGHT)),
        });
        harness.settle(128);
        // Read while the window is still open: the document goes with it.
        if let Some(handles) = grab::taken() {
            let census = Census::take(&handles);
            let mut found = panels.lock().unwrap_or_else(|held| held.into_inner());
            for title in ["Icon"] {
                if let Some(rect) = census.panel(title).and_then(|node| node.rect) {
                    found.push((title.to_owned(), rect));
                }
            }
        }
        harness.shut_down();
        Ok(())
    }
}

/// The gallery, drawn once however many assertions read it.
fn run() -> Option<&'static Opened> {
    static OPENED: std::sync::OnceLock<Option<Opened>> = std::sync::OnceLock::new();
    OPENED
        .get_or_init(|| {
            if !device::available() {
                return None;
            }
            let _guard = device::device_lock();
            grab::forget();
            let log: Log = Arc::new(Mutex::new(Vec::new()));
            let panels = Mutex::new(Vec::new());
            zgui::app()
                .with_title("gallery")
                .with_size(crate::app::WIDTH, crate::app::HEIGHT)
                .with_stylesheet(crate::shell::SHEET)
                .with_renderer(Box::new(device::factory(&log)))
                .run_on(buffers(&panels), || (Grab, view! { Gallery() }.into_view()))
                .expect("the gallery ran");
            let frames = core::mem::take(&mut *log.lock().unwrap_or_else(|held| held.into_inner()));
            assert!(!frames.is_empty(), "the gallery drew no frame at all");
            Some(Opened {
                frames,
                panels: panels.into_inner().unwrap_or_else(|held| held.into_inner()),
            })
        })
        .as_ref()
}

/// The last frame that held any vector content at all.
///
/// Not simply the last frame. Once the page has settled the application draws frames that damage
/// nothing, and their display lists are empty — so "the last one" is a frame with no drawings in
/// it, and every question asked of it comes back the same way whether the gallery draws or not.
///
/// # Panics
///
/// Panics when no frame held one, which is the answer this measurement exists to notice.
fn settled(opened: &Opened) -> &Frame {
    opened
        .frames
        .iter()
        .rev()
        .find(|frame| !frame.drawings.is_empty())
        .expect("some frame of the gallery held a drawing")
}

/// Where the panel headed `title` ended up.
///
/// # Panics
///
/// Panics when the run did not find it, because a rectangle that was never located would make every
/// assertion about what is inside it vacuously true.
fn panel(opened: &Opened, title: &str) -> Rect<DevicePx, Device> {
    opened
        .panels
        .iter()
        .find(|(name, _)| name == title)
        .map(|(_, rect)| *rect)
        .unwrap_or_else(|| panic!("the run did not find the panel headed {title:?}"))
}

/// Whether `inner` lies inside `outer`.
fn within(outer: Rect<DevicePx, Device>, inner: Rect<DevicePx, Device>) -> bool {
    inner.origin.x.0 >= outer.origin.x.0
        && inner.origin.y.0 >= outer.origin.y.0
        && inner.origin.x.0 + inner.size.width.0 <= outer.origin.x.0 + outer.size.width.0
        && inner.origin.y.0 + inner.size.height.0 <= outer.origin.y.0 + outer.size.height.0
}

#[test]
fn the_gallery_puts_its_drawings_in_the_display_list() {
    let opened = opened!();
    let frame = settled(opened);
    assert!(
        frame.drawings.len() >= 30,
        "the gallery's icons produced only {} vector items, so this is measuring the wrong \
         document rather than the wrong pixels",
        frame.drawings.len()
    );
    assert_eq!(
        frame.culled, 0,
        "the frame redraws the whole surface, so nothing may be culled out of it"
    );
}

#[test]
fn every_pass_the_gallery_planned_was_rasterised() {
    let opened = opened!();
    let frame = settled(opened);
    assert_eq!(
        frame.rasterised as usize, frame.passes,
        "the display list planned {} passes and the device rasterised {}; the difference is \
         vector content composited from a scratch nothing wrote",
        frame.passes, frame.rasterised
    );
}

#[test]
fn every_icon_on_the_gallery_s_icon_panel_marks_the_pixels_it_claimed() {
    let opened = opened!();
    let frame = settled(opened);
    let card = panel(opened, "Icon");

    let inside: Vec<_> = frame
        .drawings
        .iter()
        .filter(|drawing| within(card, drawing.ink))
        .collect();
    assert!(
        inside.len() >= 11,
        "the Icon panel holds eleven marks and three sizes; only {} drawings landed inside its \
         rectangle {card:?}",
        inside.len()
    );

    let blank: Vec<_> = inside
        .iter()
        .filter(|drawing| device::ink::fraction(&frame.pixels, drawing.ink) <= 0.0)
        .map(|drawing| (drawing.order, drawing.ink))
        .collect();
    assert!(
        blank.is_empty(),
        "{} of the Icon panel's {} drawings left their own rectangle a single flat colour, which \
         is a box of the right size with nothing in it: {blank:?}",
        blank.len(),
        inside.len()
    );
}
