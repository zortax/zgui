//! The phases that change the shape of the window rather than what is in it.
//!
//! A configure is not an interaction: nothing in the document changed, and everything in it has to
//! be laid out again anyway. `scroll-resize` is the pair of them at once, which is the case a
//! scrolled container is most likely to be wrong in — the surface changed size under content that
//! is not where the document starts.

use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::runtime::Runtime;
use zgui::vocab::PointerAction;
use zgui_platform_headless::Harness;

use crate::gallery::{HEIGHT, WIDTH};
use crate::input::{pointer, wheel};

use crate::phase::Driver;

/// Runs one of this group's phases, or answers `None` when the name is not one of them.
pub(crate) fn run(driver: &mut Driver, phase: &str) -> Option<u64> {
    let Driver {
        harness,
        scheme,
        full,
        centres: _,
        middle,
        away,
        boxes,
        fragments,
        size,
        repeats,
        ledger,
        ticks: _,
    } = driver;
    let (middle, _away) = (*middle, *away);
    let (boxes, fragments, repeats) = (*boxes, *fragments, *repeats);
    let (live_scheme, live_full) = (*scheme, std::rc::Rc::clone(full));
    let size = size.clone();
    let _ = (boxes, fragments, live_scheme, &live_full, &size);
    Some(match phase {
        "resize" => {
            let mut frames = 0;
            for index in 0..repeats {
                let width = WIDTH + (index % 24) as f32 * 8.0;
                frames += interaction!(ledger, {
                    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                        DevicePx(width),
                        DevicePx(HEIGHT),
                    )));
                    harness.settle(64)
                });
            }
            let configures = harness.app().windows()[0].surface_configures();
            println!(
                "RESIZE configures={configures} frames={frames} per_frame={:.3}",
                configures as f64 / frames.max(1) as f64
            );
            frames
        }
        // What a reader's position does when the window's extent moves under it.
        //
        // Two obligations, and they pull in opposite directions: a window that becomes shorter than
        // the offset allows has to clamp, and one that becomes taller has to leave the reader where
        // they were. Both are read off the same window, driven through a sequence of configures
        // taken while it is scrolled well into the document.
        "scroll-resize" => {
            let anchor_of = |harness: &Harness<Runtime>| {
                crate::resize::anchor(&harness.app().windows()[0], "sheet-trigger")
                    .map(|box_| box_.origin.y.0)
            };
            let report = |harness: &Harness<Runtime>, what: &str| {
                for container in crate::resize::containers(&harness.app().windows()[0]) {
                    println!("  {what:<28} {container}");
                }
                if let Some(y) = anchor_of(harness) {
                    println!("  {what:<28} anchor y={y:.1}");
                }
            };
            let sized = |harness: &mut Harness<Runtime>, width: f32, height: f32| {
                harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                    DevicePx(width),
                    DevicePx(height),
                )));
                harness.settle(96);
                harness.advance(std::time::Duration::from_micros(16_667));
                harness.settle(96);
                report(harness, &format!("after {width}x{height}"));
            };
            report(harness, "at the top");
            // What a screenful of this document is worth, before anything has been resized. The
            // count the phase ends with is compared against it, because "the offsets are all
            // legal" and "the window has something in it" are not the same claim: an offset one
            // pixel inside its limit composes the subtree one pixel below the port, and every
            // assertion about clamping passes over a window that is blank.
            live_full.wanted.set(true);
            crate::verify::repaint_everything(harness, false);
            live_full.wanted.set(false);
            let at_top = live_full
                .taken()
                .expect("a repaint against every pixel was drawn")
                .lines()
                .filter(|line| line.starts_with("  "))
                .count();
            println!("  a screenful at the top is {at_top} display-list lines");
            harness.deliver_to_first(pointer(PointerAction::Moved, middle));
            harness.settle(96);
            harness.deliver_to_first(wheel(middle, 90.0));
            harness.settle(96);
            for _ in 0..90 {
                harness.advance(std::time::Duration::from_micros(16_667));
                harness.pump();
            }
            report(harness, "scrolled down");
            sized(harness, WIDTH, 620.0);
            sized(harness, WIDTH, 1000.0);
            sized(harness, WIDTH, 300.0);
            sized(harness, WIDTH, 1000.0);
            sized(harness, 1200.0, 1000.0);
            sized(harness, WIDTH, 1000.0);
            harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
                scale_factor: 2.0,
                size: Size::new(DevicePx(WIDTH * 2.0), DevicePx(HEIGHT * 2.0)),
            });
            harness.settle(96);
            harness.advance(std::time::Duration::from_micros(16_667));
            harness.settle(96);
            report(harness, "after scale 2.0");
            harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
                scale_factor: 1.0,
                size: Size::new(DevicePx(WIDTH), DevicePx(HEIGHT)),
            });
            harness.settle(96);
            harness.advance(std::time::Duration::from_micros(16_667));
            harness.settle(96);
            report(harness, "back at scale 1.0");

            // The bottom of the document at the higher ratio, and then the lower one. An offset
            // held in device pixels is a number the smaller surface has no room for at all.
            harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
                scale_factor: 2.0,
                size: Size::new(DevicePx(WIDTH * 2.0), DevicePx(HEIGHT * 2.0)),
            });
            harness.settle(96);
            harness.deliver_to_first(wheel(middle, 400.0));
            harness.settle(96);
            for _ in 0..120 {
                harness.advance(std::time::Duration::from_micros(16_667));
                harness.pump();
            }
            report(harness, "bottom at scale 2.0");
            harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
                scale_factor: 1.0,
                size: Size::new(DevicePx(WIDTH), DevicePx(HEIGHT)),
            });
            harness.settle(96);
            harness.advance(std::time::Duration::from_micros(16_667));
            harness.settle(96);
            report(harness, "then scale 1.0");

            // The same question with no ratio in it at all. The bottom of the document in a short
            // window, and then a taller one: the content did not move, the room beneath the reader
            // grew, and the offset that was the end of the document is now past it.
            harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                DevicePx(WIDTH),
                DevicePx(800.0),
            )));
            harness.settle(96);
            harness.advance(std::time::Duration::from_micros(16_667));
            harness.settle(96);
            harness.deliver_to_first(wheel(middle, 400.0));
            harness.settle(96);
            for _ in 0..120 {
                harness.advance(std::time::Duration::from_micros(16_667));
                harness.pump();
            }
            report(harness, "bottom of a 800 tall");
            sized(harness, WIDTH, 1800.0);
            // What a viewer arriving now would be handed, which is a frame drawn against every
            // pixel rather than whatever the last one happened to answer.
            //
            // Taken from the recorder and not from the window's own scene. A scene holds one
            // frame's emission, and the frame that answers a full repaint is followed by the small
            // ones a settle produces — a caret phase, a transition ending — each of which replaces
            // it with its own handful of primitives. Reading the window afterwards therefore counts
            // the last small frame and calls a document that is drawn perfectly well a blank one.
            live_full.wanted.set(true);
            crate::verify::repaint_everything(harness, false);
            live_full.wanted.set(false);
            let transcript = live_full
                .taken()
                .expect("a repaint against every pixel was drawn");
            let drawn = transcript
                .lines()
                .filter(|line| line.starts_with("  "))
                .count();
            println!("  a full repaint here emits {drawn} display-list lines");
            let stranded = crate::resize::containers(&harness.app().windows()[0])
                .into_iter()
                .filter(|container| !container.within())
                .count();
            println!("SCROLL-RESIZE size={size} stranded={stranded} drawn={drawn} top={at_top}");
            assert_eq!(
                stranded, 0,
                "{stranded} containers are scrolled past what their content allows, so their \
                 subtrees are composed off the top of the window"
            );
            // The window at the end of the script is more than twice as tall as the one at the
            // start and is showing a position its content allows, so it cannot be drawing a
            // fraction of what one screenful drew. Stated as a fraction rather than as a number
            // because the document's size is the parameter this phase is swept over.
            assert!(
                drawn * 2 >= at_top,
                "a full repaint drew {drawn} display-list lines where a screenful at the top drew \
                 {at_top}, so most of the document is not reaching the window"
            );
            0
        }
        // A window that flipped into the dark theme against one that opened in it: the incremental
        // cascade against a first cascade, with no layout thrown away anywhere.
        _ => return None,
    })
}
