//! The velocity a touchpad release hands the compositor: the speed of the fingers as they lift.
//!
//! Each case sends a finger scroll the way a touchpad driver delivers one, and reads the release
//! from the gestures the compositor was handed.

mod support;

use std::time::Duration;

use iceblink::GesturePhase;
use zgui_geom::{Css, CssPx, Point, Size};
use zgui_platform::{Clock, SurfaceEvent};
use zgui_platform_headless::Harness;
use zgui_runtime::Runtime;
use zgui_testkit_compositor::Gestures;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerId, PointerKind, ScrollDelta, ScrollPhase, WheelEvent};

const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow: scroll }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
";

/// The rate a touchpad driver reports steps at.
const RATE_HZ: f64 = 576.0;

/// How many steps a client that reads them once per refresh of a 144 Hz output gets together.
const STEPS_PER_REFRESH: usize = 4;

/// A port of 400 rows over a compositor that records the gestures it is handed.
fn listing() -> (Harness<Runtime>, Gestures) {
    let (compositor, gestures) = zgui_testkit_compositor::recording(support::compositor());
    let harness =
        support::app_with_text_over(support::driver(compositor), CSS, |cx: &mut BuildCx<'_>| {
            let mut port = zgui_elements::column().class("port");
            for _ in 0..400 {
                port = port.child(zgui_elements::column().class("row"));
            }
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(port)
                    .into_view()
                    .build(cx),
            )
        });
    (harness, gestures)
}

/// A touchpad step of `dy` CSS pixels in `phase` over the port, stamped `at`.
fn step(dy: f64, phase: ScrollPhase, at: zgui_vocab::Timestamp) -> SurfaceEvent {
    SurfaceEvent::Wheel {
        event: WheelEvent {
            id: PointerId::MOUSE,
            kind: PointerKind::Touch,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
            delta: ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(dy as f32))),
            phase,
        },
        modifiers: Modifiers::NONE,
        timestamp: at,
    }
}

/// Sends a finger scroll whose position over time is `travel`, in CSS pixels after seconds, for
/// `duration` seconds and lifts the fingers `rest` seconds after its last step.
///
/// The steps arrive in batches of `batch`, each once its last step is due.
fn scroll(
    harness: &mut Harness<Runtime>,
    travel: impl Fn(f64) -> f64,
    duration: f64,
    rest: f64,
    batch: usize,
) {
    let surface = zgui_platform::Surface::id(harness.platform().offscreens()[0].as_ref());
    let start = harness
        .platform()
        .virtual_clock()
        .timestamp()
        .since_origin();
    let stamp = |seconds: f64| zgui_vocab::Timestamp::from_origin(start + secs(seconds));
    harness.deliver(surface, step(0.0, ScrollPhase::Started, stamp(0.0)));

    let steps = (duration * RATE_HZ).round() as usize;
    let mut arrived = Vec::new();
    for index in 1..=steps {
        let at = index as f64 / RATE_HZ;
        let previous = (index - 1) as f64 / RATE_HZ;
        let dy = travel(at) - travel(previous);
        arrived.push(step(dy, ScrollPhase::Moved, stamp(at)));
        if index % batch == 0 || index == steps {
            advance_to(harness, start, at);
            harness.deliver_all(surface, arrived.drain(..));
            harness.pump();
        }
    }

    let lift = duration + rest;
    advance_to(harness, start, lift);
    harness.deliver(surface, step(0.0, ScrollPhase::Ended, stamp(lift)));
    harness.pump();
}

/// Advances the clock of `harness` to `seconds` after `start`.
fn advance_to(harness: &mut Harness<Runtime>, start: Duration, seconds: f64) {
    let now = harness
        .platform()
        .virtual_clock()
        .timestamp()
        .since_origin();
    harness.advance((start + secs(seconds)).saturating_sub(now));
}

fn secs(seconds: f64) -> Duration {
    Duration::from_secs_f64(seconds)
}

/// The release velocity of the last gesture the compositor was handed, down the port.
fn released(gestures: &Gestures) -> f32 {
    gestures
        .handed()
        .iter()
        .rev()
        .find_map(|gesture| match gesture.phase {
            GesturePhase::End { velocity } => Some(velocity[1]),
            _ => None,
        })
        .expect("the gesture was released to the compositor")
}

/// Sends a flick that starts at 8000 px/s and slows down with a time constant of 325 ms for a
/// second, in batches of `batch` steps, and lifts the fingers one step after its last.
///
/// Returns the speed the fingers lift at and the release velocity.
fn flick(batch: usize) -> (f32, f32) {
    let (mut harness, gestures) = listing();
    harness.settle(8);
    let (initial, constant, duration) = (8000.0, 0.325, 1.0);
    let travel = |at: f64| initial * constant * (1.0 - (-at / constant).exp());
    scroll(&mut harness, travel, duration, 1.0 / RATE_HZ, batch);

    let lifting = initial * (-duration / constant).exp();
    (lifting as f32, released(&gestures))
}

#[test]
fn a_release_hands_the_speed_the_fingers_lift_at() {
    let (lifting, velocity) = flick(1);

    assert!(
        (velocity - lifting).abs() < 0.1 * lifting,
        "the fingers lifted at {lifting} px/s and the release handed {velocity}"
    );
}

#[test]
fn steps_that_arrive_together_are_measured_whole() {
    let (lifting, velocity) = flick(STEPS_PER_REFRESH);

    assert!(
        (velocity - lifting).abs() < 0.1 * lifting,
        "the fingers lifted at {lifting} px/s and the release handed {velocity}"
    );
}

#[test]
fn a_release_after_the_fingers_rested_hands_no_speed() {
    let (mut harness, gestures) = listing();
    harness.settle(8);
    scroll(&mut harness, |at| 600.0 * at, 0.5, 0.15, 1);

    assert_eq!(released(&gestures), 0.0);
}
