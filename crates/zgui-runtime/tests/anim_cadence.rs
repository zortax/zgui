//! How often a running animation actually gets a frame, on a loop that is also woken by other
//! things.
//!
//! An animation asks for no frame of its own. A window that requested one from inside every frame
//! it animated would run at whatever rate the machine could manage, which on a fast machine is a
//! core burnt to draw pictures nobody will see — so what brings the loop back for the next step is
//! a **deadline**, and the whole of an animation's frame rate is where that deadline is put.
//!
//! It has to be put at a *phase*: the moment the last frame was owed at, plus one refresh interval.
//! Derived instead as "the present moment plus one refresh interval" it is correct on precisely one
//! loop — the one that wakes for nothing but the animation — and wrong on every real one. A window
//! is woken by a great deal it did not ask for: a compositor re-stating a size, an occlusion or a
//! focus it has already reported, a pointer sample, a task finishing on another thread. Each of
//! those recomputes the park, and each one moves a deadline expressed as a delay a full interval
//! further away. Two of them per interval halve the animation's frame rate; enough of them stop it
//! altogether.
//!
//! Nothing about that is visible from inside the animation. The values are interpolated against the
//! clock, so every frame that does run holds exactly the right value and every counter agrees; the
//! only symptom is that the motion is made of a fraction of the steps the output could have shown.
//! So the cases here are written over the loop rather than over the animation: what is counted is
//! frames against elapsed time, and what is asserted on beside it is that the moment the loop is
//! parked on does not move when something unrelated happens.
//!
//! The park's other half is tested just as explicitly. An animation that finishes, and a window
//! that is hidden while one is running, must leave **no deadline at all** — a window with nothing
//! to draw that keeps waking is the same defect wearing the opposite sign.

mod support;

use std::time::Duration;

use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};

/// Cells pulsing for ever, which is the loading state every component library ships.
///
/// Infinite deliberately: a cadence is measured over a span, and an animation that ends inside the
/// span would be measured partly against a window that had correctly stopped drawing.
const PULSE_CSS: &str = "root { display: block; width: 400px; height: 300px }
                         @keyframes pulse { from { opacity: 1 } to { opacity: 0.4 } }
                         .cell { display: block; width: 40px; height: 40px;
                                 background-color: rgb(200, 200, 200);
                                 animation: pulse 2s linear infinite }";

/// A panel that fades once and then holds what it faded to, which is what `forwards` means.
const FORWARDS_CSS: &str = "root { display: block; width: 400px; height: 300px }
                            @keyframes fade { from { opacity: 1 } to { opacity: 0.2 } }
                            .panel { display: block; width: 100px; height: 100px;
                                     background-color: rgb(200, 200, 200);
                                     animation: fade 200ms linear forwards }";

/// A window on an output refreshing `millihertz` times a second, with `css` mounted and settled.
fn window_on(
    millihertz: u32,
    css: &'static str,
    class: &'static str,
) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    let mut harness = support::app(css, move |cx: &mut BuildCx<'_>| {
        let mut view = zgui_elements::column().class("root");
        for _ in 0..4 {
            view = view.child(zgui_elements::column().class(class));
        }
        Box::new(view.into_view().build(cx))
    });
    harness.platform().offscreens()[0].set_refresh_rate_millihertz(Some(millihertz));
    harness.settle(8);
    harness
}

/// One frame of an output refreshing `millihertz` times a second.
fn interval(millihertz: u32) -> Duration {
    zgui_platform::refresh_interval(Some(millihertz))
}

/// A level the compositor is re-stating, which the window is already in.
///
/// This is the unrelated wake in its purest form. It asks for no frame — a window told it is not
/// occluded when it is already not occluded has nothing to draw — and it still reaches the loop,
/// which means the park is recomputed. Everything that recomputes the park is a chance to push a
/// deadline that was expressed as a delay, and this is the one that adds nothing else at all to the
/// measurement: every frame counted below is therefore a frame the animation asked for.
fn restated() -> SurfaceEvent {
    SurfaceEvent::Occluded(false)
}

/// A configure moving the surface to `width` by `height` device pixels.
fn resized(width: f32, height: f32) -> SurfaceEvent {
    SurfaceEvent::Resized(zgui_geom::Size::new(
        zgui_geom::DevicePx(width),
        zgui_geom::DevicePx(height),
    ))
}

/// Runs `span` of virtual time with an unrelated wake arriving between ticks, and counts frames.
///
/// The step is three sevenths of a refresh interval, so a little over two wakes land inside every
/// interval — which is the rate an ordinary window sees them at.
fn frames_over(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    millihertz: u32,
    span: Duration,
) -> u64 {
    let step = interval(millihertz) * 3 / 7;
    let steps = span.as_nanos() / step.as_nanos();
    harness.reset_counts();
    let mut frames = 0;
    for _ in 0..steps {
        harness.advance(step);
        harness.deliver_to_first(restated());
        frames += harness.pump();
    }
    frames
}

#[test]
fn an_unrelated_wake_does_not_move_the_moment_the_next_tick_is_owed_at() {
    // The defect itself, asserted on where it lives rather than through its consequences. Every
    // event here asks for nothing and changes nothing; the only thing each one does is give the
    // loop a reason to work out how to park again.
    //
    // The clock moves between them, and it has to: a deadline derived as "the present moment plus
    // an interval" is only *visibly* wrong once the present moment has moved, and a run that
    // re-stated a level twelve times at a standstill would watch the wrong answer be recomputed
    // twelve times and come out the same every time.
    let millihertz = 240_000;
    let mut harness = window_on(millihertz, PULSE_CSS, "cell");
    let owed = harness
        .parked_deadline()
        .expect("the pulse installs a deadline");
    let step = interval(millihertz) / 16;

    for _ in 0..12 {
        harness.advance(step);
        harness.deliver_to_first(restated());
        assert_eq!(harness.pump(), 0, "a re-stated level bought a frame");
        assert_eq!(
            harness.parked_deadline(),
            Some(owed),
            "an unrelated wake pushed the animation's own deadline into the future; the tick was \
             owed at {owed:?} and the loop is now parked until {:?}",
            harness.parked_deadline()
        );
    }
    assert!(
        harness.now() < owed,
        "the run walked past the deadline it was watching"
    );
}

#[test]
fn an_animation_gets_one_frame_per_refresh_on_every_output() {
    // The measurement the screen shows, taken over the loop. A window on a fast output owes more
    // frames per second than one on a slow output and both owe one per refresh, so the same span of
    // virtual time is expected to produce four times as many frames at two hundred and forty hertz
    // as at sixty.
    const SPAN: Duration = Duration::from_millis(500);
    for millihertz in [240_000, 75_000, 60_000] {
        let mut harness = window_on(millihertz, PULSE_CSS, "cell");
        let frames = frames_over(&mut harness, millihertz, SPAN);
        let refreshes = SPAN.as_nanos() / interval(millihertz).as_nanos();

        // One either way: the span does not divide into a whole number of steps, and the last tick
        // of the run may be owed a moment after the run ends.
        assert!(
            frames + 1 >= refreshes as u64 && frames <= refreshes as u64 + 1,
            "an animation on a {} hz output drew {frames} frames where the output showed \
             {refreshes} — a ratio of {:.2} frames per refresh",
            millihertz / 1_000,
            frames as f64 / refreshes as f64
        );
        harness.assert_park_invariant();
    }
}

#[test]
fn a_frame_that_overran_its_interval_catches_up_without_drawing_a_backlog() {
    // A late frame is owed one frame, not one for every interval that passed while it ran. The
    // wrong answer is a deadline left in the past, which the park answers by asking for a frame
    // immediately, which leaves the next one in the past again — a burst drawn as fast as the
    // machine can draw it, all of it showing moments that have already gone.
    let millihertz = 240_000;
    let mut harness = window_on(millihertz, PULSE_CSS, "cell");
    harness.reset_counts();

    // Twelve intervals in one step: the stall a swapchain rebuild or a compositor hiccup produces.
    harness.advance(interval(millihertz) * 12);
    assert_eq!(harness.pump(), 1, "the stall was answered with a burst");
    assert_eq!(
        harness.pump(),
        0,
        "a second frame followed with nothing having asked for one"
    );
    assert!(
        harness
            .parked_deadline()
            .is_some_and(|due| due > harness.now()),
        "the deadline was left in the past, which is a spin rather than a park"
    );
    harness.assert_park_invariant();
}

#[test]
fn an_animating_window_being_resized_parks_rather_than_asking_for_frames_it_will_refuse() {
    // The two rules meet here. A window that owes a reconfiguration the output could not yet show
    // refuses *every* frame, whoever asked for it — and an animation's next moment is a phase laid
    // down before the drag began, so it falls wherever it falls, including inside that refusal.
    //
    // The loop must then wait for the moment it will start accepting frames again. Waking at a
    // moment it has already decided to refuse is not an early frame: it is a wake, a refusal, the
    // same moment computed again and another wake, for the rest of the interval, with nothing drawn.
    let millihertz = 60_000;
    let mut harness = window_on(millihertz, PULSE_CSS, "cell");
    // The clock is the test's own: a resize that crossed its own deadline by itself would take the
    // window through the very interval being set up here.
    harness.hold_clock(true);
    // And every configure marks the surface for redraw on the backend's own account, which is what
    // makes a refusal a refusal rather than an absence.
    harness.redraw_on_configure(true);
    let step = interval(millihertz);

    // One tick, so that the phase and the pacing stop coinciding: from here the animation is owed
    // at a moment no resize frame has ever run at.
    harness.advance(step);
    assert_eq!(harness.pump(), 1, "the tick produced no frame");
    let owed = harness
        .parked_deadline()
        .expect("the pulse asks to be woken again");

    // Halfway through that interval, a configure that is answered where it arrives. It starts the
    // pacing at a moment of its own and leaves the animation's moment where it was.
    harness.advance(step / 2);
    harness.deliver_to_first(resized(420.0, 320.0));
    harness.settle(8);
    assert_eq!(
        harness.parked_deadline(),
        Some(owed),
        "the resize moved the moment the animation was owed at"
    );

    // A second configure inside the same interval: recorded, not answered, and from here every
    // frame is refused until the interval that began with the frame above closes.
    harness.deliver_to_first(resized(424.0, 322.0));
    harness.pump();

    // Past the moment the animation was owed at, which is now inside the refusal.
    harness.advance(step * 3 / 5);
    assert!(harness.now() > owed, "the run stopped short of the moment");
    assert_eq!(
        harness.pump(),
        0,
        "the loop asked for a frame it had already decided to refuse"
    );
    let parked = harness.parked_deadline();
    assert!(
        parked.is_some_and(|due| due > harness.now()),
        "the loop is parked on {parked:?} at {:?}, which is a spin rather than a wait for the \
         moment it would accept a frame again",
        harness.now()
    );
    harness.assert_park_invariant();
}

#[test]
fn an_animation_that_finishes_leaves_no_deadline_and_draws_nothing_for_ten_seconds() {
    // The park policy's other half, and the one that has broken repeatedly: a window whose last
    // animation ended must stop waking. Nothing on the screen distinguishes a window parked
    // correctly from one waking sixty times a second to draw the picture it already drew.
    let millihertz = 60_000;
    let mut harness = window_on(millihertz, FORWARDS_CSS, "panel");

    // Past the end of the two-hundred-millisecond fade, one refresh at a time, so the window is
    // driven to the end the way a loop drives it rather than jumped past it.
    for _ in 0..20 {
        harness.advance(interval(millihertz));
        harness.pump();
    }
    harness.reset_counts();

    let frames = harness.run_for(Duration::from_secs(10), Duration::from_millis(16));
    assert_eq!(
        frames, 0,
        "a finished animation kept drawing {frames} frames"
    );
    assert_eq!(harness.resumes(), 0, "a finished animation kept waking");
    assert_eq!(
        harness.parked_deadline(),
        None,
        "a window with nothing to draw parked on a deadline"
    );
    harness.assert_park_invariant();
}

#[test]
fn a_window_hidden_while_animating_parks_and_starts_a_fresh_phase_when_it_is_shown() {
    // A hidden window animating at full rate is the anti-spin rule's whole reason for existing, and
    // the phase it was on is worth nothing when it comes back: an occlusion is not bounded by
    // anything, so the first interval after one has to be measured from the frame that follows it.
    let millihertz = 60_000;
    let mut harness = window_on(millihertz, PULSE_CSS, "cell");
    assert!(harness.parked_deadline().is_some(), "the pulse is running");

    harness.deliver_to_first(SurfaceEvent::Occluded(true));
    harness.settle(8);
    assert_eq!(
        harness.parked_deadline(),
        None,
        "a hidden window kept a deadline for an animation nobody can see"
    );

    harness.reset_counts();
    let frames = harness.run_for(Duration::from_secs(5), Duration::from_millis(16));
    assert_eq!(frames, 0, "a hidden window drew {frames} frames");
    harness.assert_park_invariant();

    harness.deliver_to_first(SurfaceEvent::Occluded(false));
    harness.settle(8);
    let shown = harness.now();
    let due = harness
        .parked_deadline()
        .expect("the pulse asks to be woken again");
    assert!(
        due > shown && due <= shown + interval(millihertz),
        "the first interval after five seconds of occlusion was {:?} rather than one refresh",
        due.saturating_duration_since(shown)
    );

    // And it goes on at the rate of the output rather than at whatever the resume left behind.
    let frames = frames_over(&mut harness, millihertz, Duration::from_millis(500));
    let refreshes = Duration::from_millis(500).as_nanos() / interval(millihertz).as_nanos();
    assert!(
        frames + 1 >= refreshes as u64,
        "a window shown again drew {frames} frames against {refreshes} refreshes"
    );
    harness.assert_park_invariant();
}
