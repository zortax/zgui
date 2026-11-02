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
//! pipeline is compiled, no surface is configured and no driver shader cache is read — which is why
//! the number here is smaller than the same gallery opening on a real desktop. The difference is
//! the device, and it is left out deliberately: what a driver takes to compile a pipeline is a
//! property of the machine and its graphics stack, it varies by more than everything measured here
//! put together, and a band around it would fire on a driver update rather than on a change to this
//! framework. The budget the schedule wrote — under 250 ms warm — is checked against this number,
//! which means it is checked with room to spare rather than exactly.
//!
//! # Which font cache state it measures
//!
//! A warm one. Font discovery reads an on-disk cache (`ZGUI_FONT_CACHE`), and a start that finds no
//! valid file takes two to three times as long. The sweep gives every scenario a private cache
//! directory next to its binary and runs this scenario once before it measures, so the measured
//! start always finds a valid file, and the user's own cache has no effect. Set `ZGUI_FONT_CACHE`
//! before the sweep (for example to `off`) to measure another state. Glyphs for the first frame are
//! rasterised on the layout pool, so the number also depends on how many cores the process gets.

use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;

use crate::scenario::band::{Band, Measurement, Pace, SCHEDULER_TOLERANCE, Spread};
use crate::scenario::{Outcome, counters};

/// Runs the scenario.
pub(crate) fn run() -> Outcome {
    let before = zgui_profile::counter::snapshot();
    let gpu = crate::gpu_time::phase("cold.first_frame");
    let started = std::time::Instant::now();

    let mut harness = zgui_platform_headless::Harness::new(crate::gallery::runtime("s13"));
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(crate::gallery::WIDTH),
        DevicePx(crate::gallery::HEIGHT),
    )));
    harness.settle(256);
    let elapsed = started.elapsed();
    drop(gpu);
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
                baseline: 53.0,
                tolerance: SCHEDULER_TOLERANCE,
            },
            rationale: "measured at 49-66 ms headless across twelve starts with a warm font \
                        cache, and 112-163 ms with the cache off; the first frame's glyphs are \
                        rasterised on the layout pool, so the cores the process gets set the \
                        spread",
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
