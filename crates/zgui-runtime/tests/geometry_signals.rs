//! Which readers a configure wakes.
//!
//! The window handle reports its size and its density as two signals, and a configure carries
//! both. A drag is hundreds of configures that move the width alone, so the two must be written
//! apart: an effect reading the scale — and application code reads the scale wherever it converts
//! a length — must sleep through a drag, and wake exactly when the density moves.

mod support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use zgui_geom::{Device, DevicePx, Size};
use zgui_platform::SurfaceEvent;
use zgui_reactive::prelude::Get;
use zgui_view::{BuildCx, IntoView, View};

/// A surface extent of `width` by a fixed height.
fn wide(width: f32) -> Size<DevicePx, Device> {
    Size::new(DevicePx(width), DevicePx(300.0))
}

/// An effect counting its own runs, kept for the life of the test.
fn counting<T: Clone + Send + Sync + 'static>(signal: zgui_reactive::Signal<T>) -> Rc<Cell<u32>> {
    let runs = Rc::new(Cell::new(0));
    let seen = Rc::clone(&runs);
    core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
        let _ = signal.get();
        seen.set(seen.get() + 1);
    }));
    runs
}

#[test]
fn a_width_step_wakes_the_readers_of_the_width_alone() {
    let mut harness = support::app("", |cx: &mut BuildCx<'_>| {
        Box::new(zgui_elements::column().into_view().build(cx))
    });
    harness.deliver_to_first(SurfaceEvent::Resized(wide(400.0)));
    harness.settle(8);

    let handle = harness.app().windows()[0].handle().clone();
    let sizes = counting(handle.size());
    let scales = counting(handle.scale());
    harness.settle(8);
    let sizes_before = sizes.get();
    let scales_before = scales.get();

    // A drag of width-only steps, each far enough from the last that it is answered where it
    // arrives rather than deferred into a coalesced frame.
    for step in 1..=8u32 {
        harness.advance(Duration::from_millis(25));
        #[expect(
            clippy::cast_precision_loss,
            reason = "a step index bounded by the loop above"
        )]
        harness.deliver_to_first(SurfaceEvent::Resized(wide(400.0 + step as f32 * 10.0)));
        harness.settle(8);
    }

    assert_eq!(
        sizes.get() - sizes_before,
        8,
        "eight width steps woke the size readers {} times",
        sizes.get() - sizes_before
    );
    assert_eq!(
        scales.get(),
        scales_before,
        "a drag that never moved the density woke the scale readers {} times",
        scales.get() - scales_before
    );

    // The density moving is still reported — the comparison suppresses repeats and nothing else.
    harness.advance(Duration::from_millis(25));
    harness.deliver_to_first(SurfaceEvent::ScaleFactorChanged {
        scale_factor: 2.0,
        size: wide(960.0),
    });
    harness.settle(8);
    assert_eq!(
        scales.get(),
        scales_before + 1,
        "a density change woke the scale readers {} times",
        scales.get() - scales_before
    );

    harness.shut_down();
}
