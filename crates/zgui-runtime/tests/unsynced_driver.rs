//! A window on a stepped driver on a thread of its own, which returns from each step before the
//! driver built it.

mod support;

use std::time::Duration;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::Harness;
use zgui_profile::Counter;
use zgui_runtime::Runtime;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{
    Modifiers, PointerId, PointerKind, ScrollDelta, ScrollPhase, Timestamp, WheelEvent,
};

const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow: scroll }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
";

/// The refresh interval the headless platform paces frames at.
const TICK: Duration = Duration::from_millis(8);

/// The window frames and compositor ticks of each tick.
type Ticks = Vec<(u64, u64)>;

/// A port of `rows` rows over the driver `client`.
fn listing(client: Box<dyn compositor_api::CompositorClient>, rows: usize) -> Harness<Runtime> {
    support::app_with_text_over(client, CSS, move |cx: &mut BuildCx<'_>| {
        let mut port = zgui_elements::column().class("port");
        for _ in 0..rows {
            port = port.child(zgui_elements::column().class("row"));
        }
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(port)
                .into_view()
                .build(cx),
        )
    })
}

/// Delivers a wheel notch of `lines` lines over the port.
fn notch(harness: &mut Harness<Runtime>, lines: f32) {
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: PointerId::MOUSE,
            kind: PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
            delta: ScrollDelta::Lines { x: 0.0, y: lines },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
}

/// Scrolls the port by three wheel notches of `lines` lines from rest, and returns the window
/// frames and compositor ticks of every tick of the glides.
///
/// `between` runs before every turn of the loop.
fn glide(mut harness: Harness<Runtime>, lines: f32, mut between: impl FnMut()) -> Ticks {
    for _ in 0..8 {
        between();
        harness.advance(TICK);
        harness.pump();
    }
    let mut ticks = Vec::new();
    for _ in 0..3 {
        between();
        notch(&mut harness, lines);
        harness.settle(64);
        for _ in 0..60 {
            between();
            let before = zgui_profile::counter::snapshot();
            harness.advance(TICK);
            harness.pump();
            let moved = before.delta(&zgui_profile::counter::snapshot());
            ticks.push((
                moved.get(Counter::ZguiFrames),
                moved.get(Counter::CompositorTicks),
            ));
        }
    }
    ticks
}

/// Runs [`glide`] over `rows` rows by notches of `lines` lines, on a driver that answers every
/// step before it returns, and on a stepped driver on a thread of its own that is waited for
/// only between the turns of the loop.
fn both(rows: usize, lines: f32) -> (Ticks, Ticks) {
    let answered = glide(listing(support::client(), rows), lines, || {});
    let (client, mut waiter) = zgui_testkit_compositor::unsynced_thread(support::compositor());
    let unsynced = glide(listing(client, rows), lines, || waiter.wait());
    (answered, unsynced)
}

#[test]
fn a_glide_runs_the_frames_it_runs_on_a_driver_that_answers_each_step_before_it_returns() {
    let _turn = zgui_profile::counter::exclusive();
    // Past the recorded rows, so the compositor wakes the window, and within them, so nothing
    // does until the glide rests.
    for (rows, lines) in [(200, 6.0), (10, 1.0)] {
        let (answered, unsynced) = both(rows, lines);
        let ticks: u64 = answered.iter().map(|(_, ticks)| ticks).sum();
        assert!(
            ticks > 3,
            "{rows} rows: the glides ticked the compositor {ticks} times"
        );
        assert_eq!(unsynced, answered, "{rows} rows");
    }
}
