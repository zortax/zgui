//! What a pending frame callback buys, driven through a real window.
//!
//! A frame callback is the seam a view drives an animation of its own through: register, step,
//! register again. The properties asserted here are the loop's, and none is visible from inside
//! the callback. A pending registration must make the window count as animating, so the frames
//! that run it come at the output's refresh interval rather than at a rate a component guessed;
//! a callback that stops registering must leave no deadline at all; and a cancelled one must
//! neither run nor keep the loop waking.

mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use zgui_platform::SurfaceEvent;
use zgui_runtime::RuntimeHost;
use zgui_view::{BuildCx, IntoView, View, ViewHost};
use zgui_vocab::Timestamp;

/// A static block, so the window has something to show and nothing that animates on its own.
const STILL_CSS: &str = "root { display: block; width: 400px; height: 300px }
                         .block { display: block; width: 100px; height: 100px;
                                  background-color: rgb(200, 200, 200) }";

/// A window on an output refreshing `millihertz` times a second, settled.
fn window_on(millihertz: u32) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    let mut harness = support::app(STILL_CSS, move |cx: &mut BuildCx<'_>| {
        let view = zgui_elements::column()
            .class("root")
            .child(zgui_elements::r#box().class("block"))
            .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.platform().offscreens()[0].set_refresh_rate_millihertz(Some(millihertz));
    harness.settle(8);
    harness
}

/// One frame of an output refreshing `millihertz` times a second.
fn interval(millihertz: u32) -> Duration {
    zgui_platform::refresh_interval(Some(millihertz))
}

/// The window's host, cloned out so the harness can be driven while it is held.
fn host_of(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Rc<RuntimeHost> {
    Rc::clone(harness.app().windows()[0].host())
}

#[test]
fn a_registration_made_outside_any_frame_buys_the_frame_itself() {
    // The moment of the registration is the deadline that wakes the loop: nothing else here asks
    // for a frame, so a registration that bought none would sit pending for ever.
    let mut harness = window_on(60_000);
    let host = host_of(&harness);
    let seen: Rc<Cell<Option<Timestamp>>> = Rc::default();

    let report = Rc::clone(&seen);
    host.request_frame_callback(Rc::new(move |at| report.set(Some(at))));
    assert!(
        harness.app().windows()[0].is_animating(),
        "a pending frame callback is an animation by the cadence's measure"
    );

    harness.settle(4);
    let origin = {
        use zgui_platform::Clock;
        harness.platform().virtual_clock().origin()
    };
    assert_eq!(
        seen.get(),
        Some(Timestamp::from_origin(
            harness.now().saturating_duration_since(origin)
        )),
        "the callback runs with the moment the frame is for"
    );
    assert!(
        !harness.app().windows()[0].is_animating(),
        "a callback that did not register again leaves nothing pending"
    );
    harness.reset_counts();
    let frames = harness.run_for(Duration::from_secs(2), Duration::from_millis(16));
    assert_eq!(
        frames, 0,
        "a spent registration bought {frames} more frames"
    );
    assert_eq!(
        harness.parked_deadline(),
        None,
        "a window with nothing left to run parked on a deadline"
    );
    harness.assert_park_invariant();
}

#[test]
fn a_re_registering_callback_runs_once_per_refresh_on_every_output() {
    // The whole point of the seam: the heartbeat is paced by the output, so the same span of
    // virtual time runs four times as many steps at two hundred and forty hertz as at sixty.
    const SPAN: Duration = Duration::from_millis(500);
    for millihertz in [240_000, 60_000] {
        let mut harness = window_on(millihertz);
        let host = host_of(&harness);
        let runs = Rc::new(Cell::new(0u64));
        let stamps: Rc<RefCell<Vec<Timestamp>>> = Rc::default();

        // The knot a self-driven animation ties: the callback holds what it needs to register
        // its own successor.
        struct Beat {
            host: Rc<RuntimeHost>,
            runs: Rc<Cell<u64>>,
            stamps: Rc<RefCell<Vec<Timestamp>>>,
        }
        fn arm(beat: &Rc<Beat>) {
            let again = Rc::clone(beat);
            beat.host.request_frame_callback(Rc::new(move |at| {
                again.runs.set(again.runs.get() + 1);
                again.stamps.borrow_mut().push(at);
                arm(&again);
            }));
        }
        arm(&Rc::new(Beat {
            host,
            runs: Rc::clone(&runs),
            stamps: Rc::clone(&stamps),
        }));

        // A little over two wakes per interval, with an unrelated wake between ticks — the rate
        // an ordinary window is woken at.
        let step = interval(millihertz) * 3 / 7;
        let steps = SPAN.as_nanos() / step.as_nanos();
        for _ in 0..steps {
            harness.advance(step);
            harness.deliver_to_first(SurfaceEvent::Occluded(false));
            harness.pump();
        }

        let refreshes = (SPAN.as_nanos() / interval(millihertz).as_nanos()) as u64;
        assert!(
            runs.get() + 1 >= refreshes && runs.get() <= refreshes + 2,
            "a callback on a {} hz output ran {} times where the output showed {refreshes} \
             refreshes",
            millihertz / 1_000,
            runs.get()
        );
        assert!(
            stamps.borrow().windows(2).all(|pair| pair[0] < pair[1]),
            "the moments handed to successive runs go backwards: {:?}",
            stamps.borrow()
        );
        harness.assert_park_invariant();
    }
}

#[test]
fn a_callback_that_stops_registering_lets_the_window_park() {
    let mut harness = window_on(60_000);
    let host = host_of(&harness);
    let runs = Rc::new(Cell::new(0u64));

    struct Beat {
        host: Rc<RuntimeHost>,
        runs: Rc<Cell<u64>>,
    }
    fn arm(beat: &Rc<Beat>) {
        let again = Rc::clone(beat);
        beat.host.request_frame_callback(Rc::new(move |_| {
            again.runs.set(again.runs.get() + 1);
            if again.runs.get() < 3 {
                arm(&again);
            }
        }));
    }
    arm(&Rc::new(Beat {
        host,
        runs: Rc::clone(&runs),
    }));

    for _ in 0..10 {
        harness.advance(interval(60_000));
        harness.pump();
    }
    assert_eq!(runs.get(), 3, "one run per frame, stopping by not asking");

    harness.reset_counts();
    let frames = harness.run_for(Duration::from_secs(5), Duration::from_millis(16));
    assert_eq!(
        frames, 0,
        "a finished heartbeat kept drawing {frames} frames"
    );
    assert_eq!(
        harness.parked_deadline(),
        None,
        "a finished heartbeat left a deadline behind"
    );
    harness.assert_park_invariant();
}

#[test]
fn a_cancelled_callback_never_runs_and_leaves_no_deadline() {
    let mut harness = window_on(60_000);
    let host = host_of(&harness);
    let runs = Rc::new(Cell::new(0u64));

    let counter = Rc::clone(&runs);
    let id = host.request_frame_callback(Rc::new(move |_| counter.set(counter.get() + 1)));
    host.cancel_frame_callback(id);
    assert!(
        !harness.app().windows()[0].is_animating(),
        "a cancelled registration still counts as animating"
    );

    harness.reset_counts();
    let frames = harness.run_for(Duration::from_secs(2), Duration::from_millis(16));
    assert_eq!(runs.get(), 0, "a cancelled callback ran");
    assert_eq!(frames, 0, "a cancelled callback bought {frames} frames");
    assert_eq!(harness.parked_deadline(), None);
    harness.assert_park_invariant();
}

#[test]
fn an_occluded_window_holds_its_callbacks_and_runs_them_when_it_is_shown() {
    // The anti-spin rule: a heartbeat behind a hidden window must cost nothing, and the
    // registration survives to run on the first frame after the window is shown again.
    let mut harness = window_on(60_000);
    harness.deliver_to_first(SurfaceEvent::Occluded(true));
    harness.settle(8);

    let host = host_of(&harness);
    let runs = Rc::new(Cell::new(0u64));
    let counter = Rc::clone(&runs);
    host.request_frame_callback(Rc::new(move |_| counter.set(counter.get() + 1)));

    harness.reset_counts();
    let frames = harness.run_for(Duration::from_secs(5), Duration::from_millis(16));
    assert_eq!(frames, 0, "a hidden window drew {frames} frames");
    assert_eq!(runs.get(), 0, "a hidden window ran a frame callback");
    assert_eq!(
        harness.parked_deadline(),
        None,
        "a hidden window kept a deadline for a callback nobody can see"
    );

    harness.deliver_to_first(SurfaceEvent::Occluded(false));
    harness.settle(8);
    assert_eq!(
        runs.get(),
        1,
        "the held callback runs once the window is shown"
    );
    harness.assert_park_invariant();
}
