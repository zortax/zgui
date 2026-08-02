//! Cold start: from nothing to a window with the whole gallery painted in it.
//!
//! The only scenario whose clock is the wall's. Everything else runs on the harness's clock, so a
//! number is CPU work and nothing else; here the question *is* elapsed time, because what is being
//! measured is what a person waits through between asking for the program and seeing it. Font
//! enumeration, sheet parsing, the first cascade over a document that has no previous style to
//! reuse, the first layout with no cached measurement, and the first frame's whole emission are all
//! inside it, and every one of them is a place where a cache that used to be warm can stop being.
//!
//! Measured once, not repeated: the second start in a process is not cold. The font stack, the
//! interned names and the process's own pages are all warm by then, so a median over repeats would
//! be a median of warm starts wearing this scenario's name.
//!
//! # What is not in it
//!
//! The graphics device. This runs against the headless platform, so no adapter is enumerated, no
//! pipeline is compiled and no surface is configured — which is why the number here is about a
//! hundred milliseconds while the same gallery opening on a real desktop takes 216–247 ms. The
//! difference is the device, and it is left out deliberately: what a driver takes to compile a
//! pipeline is a property of the machine and its graphics stack, it varies by more than everything
//! measured here put together, and a band around it would fire on a driver update rather than on a
//! change to this framework. The budget the schedule wrote — under 250 ms warm — is checked against
//! this number, which means it is checked with room to spare rather than exactly.

use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;

use crate::scenario::band::{Band, Measurement, Pace, STARTUP_TOLERANCE, Spread};
use crate::scenario::{Outcome, counters};

/// Runs the scenario.
pub(crate) fn run() -> Outcome {
    let before = zgui_profile::counter::snapshot();
    let started = std::time::Instant::now();

    let mut harness = zgui_platform_headless::Harness::new(crate::gallery::runtime("s13"));
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(crate::gallery::WIDTH),
        DevicePx(crate::gallery::HEIGHT),
    )));
    harness.settle(256);
    let elapsed = started.elapsed();
    let moved = before.delta(&zgui_profile::counter::snapshot());

    let boxes = harness.app().windows()[0].layout().borrow().keys().len();
    assert!(
        moved.primitives_emitted > 0,
        "the first frame emitted no primitive, so this measured a window that never painted"
    );

    let mut sample = [elapsed.as_secs_f64() * 1e3];
    let pace = Pace::of(&sample, 16.667);
    let first = Spread::of(&mut sample);

    Outcome {
        scenario: "cold-start",
        document: format!("gallery s13, {boxes} boxes, first painted frame"),
        measurements: vec![Measurement {
            name: "cold.first_frame",
            unit: "ms",
            value: first.p50,
            band: Band::Time {
                baseline: 112.0,
                tolerance: STARTUP_TOLERANCE,
            },
            rationale: "measured at 102-111 ms headless; on screen with a device it is 216-247 ms",
            budget: Some(250.0),
            spread: Some(first),
        }]
        .into_iter()
        .chain(crate::scenario::band::whole_document_reshape(&moved))
        .collect(),
        counters: counters(&moved),
        notes: vec![
            "this distribution is taken over a population of one, and it has to be: the second \
             start in a process is a warm start wearing this scenario's name, so there is no \
             second sample to take. Read the four figures as the one launch they are"
                .to_owned(),
        ],
        pace,
    }
}
