//! A transparent window, composed on a real device into a translucent target.
//!
//! A window that asked for a transparent surface is composited over something else, so every
//! pixel it composes has to be a premultiplied colour: a pixel nothing paints has zero alpha, a
//! half-transparent background keeps half its alpha, and text has ordinary coverage, because
//! per-channel coverage writes no alpha at all. The same document in an opaque window is the
//! control that shows the target follows the window's attributes.
//!
//! The opaque window also shows that its text antialiasing setting decides the coverage of its
//! text and the side of a stem each colour channel is on.

use std::sync::{Arc, Mutex};

use compositor_types::{BufferAge, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget};
use zgui::platform::{AppHandler, PlatformError, SurfaceEvent};
use zgui::prelude::*;
use zgui_platform_headless::Harness;
use zgui_testkit_compositor::{Composed, Device, Pixels, Shown};

/// The window's extent, in device pixels at scale one.
const WIDTH: i32 = 200;
/// The window's extent, in device pixels at scale one.
const HEIGHT: i32 = 100;

const SHEET: &str = zgui::css!(
    ".root { display: flex; flex-direction: row; width: 200px; height: 100px }
     .panel {
        width: 100px;
        height: 100px;
        flex-shrink: 0;
        background-color: rgba(255, 0, 0, 0.5);
     }
     .label {
        margin-left: 10px;
        margin-top: 10px;
        color: #ffffff;
        font-size: 24px;
     }"
);

/// What one frame produced.
struct Frame {
    /// The composed target, read back.
    pixels: Arc<Pixels>,
    /// How many glyphs were drawn with per-channel coverage.
    subpixel: usize,
    /// How many glyphs were drawn with ordinary coverage.
    mono: usize,
    /// Whether the device can blend per-channel coverage.
    capable: bool,
}

/// What one run recorded.
#[derive(Default)]
struct Record {
    /// Whether each target the renderer was configured with was opaque, in order.
    targets: Vec<bool>,
    /// Every frame, in order.
    frames: Vec<Frame>,
}

/// The device, recording each frame.
struct Recording {
    /// The device.
    device: Device,
    /// What the device's last frame drew.
    shown: Shown,
    /// The device's composed target.
    composed: Composed,
    /// Where frames go.
    record: Arc<Mutex<Record>>,
}

impl compositor_api::Renderer for Recording {
    fn capabilities(&self) -> RenderCapabilities {
        self.device.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        lock(&self.record).targets.push(target.opaque);
        self.device.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.device.target()
    }

    fn target_age(&self) -> BufferAge {
        self.device.target_age()
    }

    fn draw(&mut self, frame: &compositor_types::Frame) -> FrameOutcome {
        let outcome = self.device.draw(frame);
        let drawing = self.shown.drawing().expect("the device recorded the frame");
        lock(&self.record).frames.push(Frame {
            pixels: self
                .composed
                .pixels()
                .expect("the device read its target back"),
            subpixel: drawing.of_kind("subpixel").count(),
            mono: drawing.of_kind("mono").count(),
            capable: self.device.capabilities().subpixel_text,
        });
        outcome
    }

    fn set_subpixel_order(&mut self, order: compositor_types::SubpixelOrder) {
        self.device.set_subpixel_order(order);
    }

    fn memory(&self) -> MemoryReport {
        self.device.memory()
    }
}

/// Locks `record`, whether or not an earlier assertion poisoned it.
fn lock(record: &Mutex<Record>) -> std::sync::MutexGuard<'_, Record> {
    record.lock().unwrap_or_else(|held| held.into_inner())
}

/// Returns `true` where this machine has a device these tests can draw on.
fn available() -> bool {
    match Device::open([8, 8]) {
        Ok(_) => true,
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            false
        }
    }
}

/// Keeps the tests in this binary off the device one at a time.
static DEVICE: Mutex<()> = Mutex::new(());

/// Runs the document in a window that is transparent or not, and returns what it recorded.
fn run(transparent: bool) -> Record {
    run_with(transparent, zgui::TextAntialiasing::Auto, SHEET)
}

/// Runs the document with a stated text antialiasing, and returns what it recorded.
fn run_with(transparent: bool, antialiasing: zgui::TextAntialiasing, sheet: &str) -> Record {
    let _device = DEVICE.lock().unwrap_or_else(|held| held.into_inner());
    let record = Arc::new(Mutex::new(Record::default()));
    let factory_record = Arc::clone(&record);
    let factory = move |size: [u32; 2]| {
        let device = Device::open(size).expect("the device opened a moment ago");
        Box::new(Recording {
            shown: device.shown(),
            composed: device.composed(),
            device,
            record: Arc::clone(&factory_record),
        }) as Box<dyn compositor_api::Renderer>
    };
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
        .with_title("translucent")
        .with_size(WIDTH as f32, HEIGHT as f32)
        .with_transparent(transparent)
        .with_text_antialiasing(antialiasing)
        .with_stylesheet(sheet)
        .with_compositor_renderer(Box::new(factory))
        .run_on(drive, || {
            view! {
                box(class = "root") {
                    box(class = "panel") {}
                    box(class = "label") { text { "Wg" } }
                }
            }
        })
        .expect("the application ran");
    Arc::into_inner(record)
        .map(|held| held.into_inner().unwrap_or_else(|held| held.into_inner()))
        .expect("the renderer was dropped with the application")
}

/// Returns `true` where `a` and `b` differ by at most one.
fn near(a: u8, b: u8) -> bool {
    a.abs_diff(b) <= 1
}

#[test]
fn a_transparent_window_composes_premultiplied_pixels_with_ordinary_text_coverage() {
    if !available() {
        return;
    }
    let record = run(true);
    assert!(
        !record.targets.is_empty() && record.targets.iter().all(|opaque| !opaque),
        "every target of a transparent window is translucent: {:?}",
        record.targets
    );
    let frame = record.frames.last().expect("the window drew a frame");
    assert!(frame.mono > 0, "the label was drawn");
    assert_eq!(
        frame.subpixel, 0,
        "per-channel coverage has no alpha, so a translucent target never gets it"
    );

    let panel = frame.pixels.rgba(50, 50);
    assert!(
        near(panel[0], 128) && panel[1] == 0 && panel[2] == 0 && near(panel[3], 128),
        "half-transparent red keeps half its alpha, premultiplied: {panel:?}"
    );
    assert_eq!(
        frame.pixels.rgba(WIDTH - 2, HEIGHT - 2),
        [0, 0, 0, 0],
        "a pixel nothing paints is fully transparent"
    );

    let mut inked = 0;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let [r, g, b, a] = frame.pixels.rgba(x, y);
            assert!(
                r.max(g).max(b) <= a.saturating_add(1),
                "({x}, {y}) is not a premultiplied colour: {:?}",
                [r, g, b, a]
            );
            if x >= 110 && a > 0 {
                inked += 1;
            }
        }
    }
    assert!(inked > 0, "the label drew with alpha");
}

#[test]
fn an_opaque_window_keeps_opaque_targets() {
    if !available() {
        return;
    }
    let record = run(false);
    assert!(
        !record.targets.is_empty() && record.targets.iter().all(|opaque| *opaque),
        "an ordinary window is opaque: {:?}",
        record.targets
    );
    let frame = record.frames.last().expect("the window drew a frame");
    assert!(frame.mono + frame.subpixel > 0, "the label was drawn");
}

/// How far the ink in the red channel sits to the right of the ink in the blue channel, in
/// pixels, over the label of a frame drawn on a black page.
fn red_lead(frame: &Frame) -> f32 {
    let centre = |channel: usize| {
        let (mut weight, mut moment) = (0.0, 0.0);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let ink = f32::from(frame.pixels.rgba(x, y)[channel]);
                weight += ink;
                moment += ink * x as f32;
            }
        }
        moment / weight
    };
    centre(0) - centre(2)
}

/// The stated antialiasing decides the coverage of an opaque window on a device that can blend
/// per-channel coverage, and the stated order decides which side of a stem each channel is on.
///
/// The red stripe of a pixel is on the left for one order and on the right for the other. So the
/// red ink of white text leads the blue ink for one order, and trails it by the same distance for
/// the other.
#[test]
fn an_opaque_window_draws_the_coverage_its_antialiasing_states() {
    if !available() || std::env::var_os("ZGUI_TEXT_AA").is_some() {
        return;
    }
    let sheet = format!("{SHEET} .root {{ background-color: #000000 }} .panel {{ display: none }}");
    let grey = run_with(false, zgui::TextAntialiasing::Grayscale, &sheet);
    let frame = grey.frames.last().expect("the window drew a frame");
    assert!(frame.mono > 0, "the label was drawn");
    assert_eq!(frame.subpixel, 0, "grayscale text has no per-channel glyph");
    assert!(
        red_lead(frame).abs() < 0.05,
        "one coverage for each pixel tints nothing: {}",
        red_lead(frame)
    );

    let striped = run_with(false, zgui::TextAntialiasing::Rgb, &sheet);
    let frame = striped.frames.last().expect("the window drew a frame");
    if !frame.capable {
        eprintln!("skipped: the device draws no per-channel coverage");
        return;
    }
    assert!(frame.subpixel > 0, "the label has per-channel coverage");
    assert_eq!(frame.mono, 0, "no glyph of the label has ordinary coverage");
    let lead = red_lead(frame);
    assert!(lead > 0.1, "red leads blue by {lead}");

    let reversed = run_with(false, zgui::TextAntialiasing::Bgr, &sheet);
    let frame = reversed.frames.last().expect("the window drew a frame");
    assert!(
        (red_lead(frame) + lead).abs() < 0.05,
        "red trails blue by {}",
        red_lead(frame)
    );
}
