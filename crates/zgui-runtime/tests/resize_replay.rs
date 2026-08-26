//! What a resize leaves standing in the paint cache.
//!
//! A resize moves the scrollport's clip, and the clip is named by the box that imposes it rather
//! than by the rectangle it happens to hold this frame — so every record under it keeps naming a
//! chain that still exists, and content the resize did not touch replays instead of encoding
//! again. This is the difference between a resize frame that costs the changed boxes and one that
//! costs the document.
//!
//! The fixture is the shape the cost report arrives in: rows of fixed-width text inside a
//! scrolling root. A height-only step re-wraps nothing and moves nothing that is anchored to the
//! top, so everything those rows painted is still exactly right — the frame owes the resized
//! root's own fragments and no more.
//!
//! The counters are a process-wide block, so this is one test in a target of its own.

mod support;

use zgui_geom::{Device, DevicePx, Size};
use zgui_platform::SurfaceEvent;
use zgui_profile::{COUNTERS_ENABLED, Counter};
use zgui_testkit_scene::counters::Recording;

/// How many rows the document holds.
///
/// Enough that the log overflows its port at every height this test configures, so the gutter
/// decision never flips: a flip rebuilds the scroll container's boxes, and rebuilt boxes carry
/// rebuilt fragments whatever the paint cache does.
const ROWS: usize = 40;

/// A window-filling root holding a window-filling scrollport full of fixed-width rows.
const CSS: &str = "
root { display: block; width: 100%; height: 100%; background-color: #101010 }
.log { display: block; width: 100%; height: 100%; overflow-y: auto }
text { display: block; width: 200px }
";

/// The window under test.
fn window() -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let mut log = zgui_elements::column().class("log");
        for row in 0..ROWS {
            log = log.child(zgui_elements::text().child(format!("row {row} says a few words")));
        }
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(log)
                .into_view()
                .build(cx),
        )
    })
}

/// A surface extent of a fixed width by `height`.
fn tall(height: f32) -> Size<DevicePx, Device> {
    Size::new(DevicePx(400.0), DevicePx(height))
}

#[test]
fn a_height_resize_of_a_scrolling_document_replays_its_rows() {
    let mut recording = Recording::begin();
    let mut harness = window();

    // The control: building the document encodes every row, which is what makes the ceiling at
    // the bottom mean "the resize re-encoded almost nothing" rather than "nothing was measured".
    let built = recording.measure(|| {
        harness.deliver_to_first(SurfaceEvent::Resized(tall(300.0)));
        harness.settle(64);
    });

    // One resize before the measured one: the first resize a freshly opened window takes carries
    // one-time work no later step repeats, and the measurement is of a drag in progress.
    harness.deliver_to_first(SurfaceEvent::Resized(tall(330.0)));
    harness.settle(64);

    // The step being pinned: the window grows taller. Nothing re-wraps — the rows' width is
    // pinned — and nothing anchored to the top moves.
    let step = recording.measure(|| {
        harness.deliver_to_first(SurfaceEvent::Resized(tall(360.0)));
        harness.settle(64);
    });

    if !COUNTERS_ENABLED {
        return;
    }

    assert!(
        built.get(Counter::ChunksReencoded) >= ROWS as u64,
        "building the document encoded {} chunks against {ROWS} rows, so the fixture is not the \
         row-per-record document the assertions below reason about",
        built.get(Counter::ChunksReencoded),
    );
    assert_eq!(
        step.get(Counter::BoxesRebuilt),
        0,
        "the step rebuilt {} boxes; a rebuilt box carries rebuilt fragments, and their fresh \
         names would make the replay claim below vacuous",
        step.get(Counter::BoxesRebuilt),
    );
    assert!(
        step.get(Counter::ChunksTranslated) >= ROWS as u64,
        "the resize replayed {} chunks against {ROWS} rows: the rows were re-encoded, so the \
         scrollport's clip was minted afresh instead of keeping its name",
        step.get(Counter::ChunksTranslated),
    );
    assert!(
        step.get(Counter::ChunksReencoded) <= 8,
        "the resize re-encoded {} chunks; a height step owes the resized boxes' own fragments \
         and the scrollbar that measures them, and nothing that scrolls inside",
        step.get(Counter::ChunksReencoded),
    );
}
