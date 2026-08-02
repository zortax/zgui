//! How often the picture actually changes while a scroll is moving on its own.
//!
//! An animation's cadence has two halves and only the first is about the loop. The first is whether
//! the window *runs* a frame per refresh of the output it is on, which is the deadline's job and is
//! asserted in [`anim_cadence`](../anim_cadence/index.html). The second is whether those frames
//! **differ from one another** — because a frame whose picture is identical to the last one damages
//! nothing, and a renderer refuses an undamaged frame rather than spending a swap-chain image on
//! pixels the surface already holds. So a window can run a frame per refresh and still show a
//! motion made of half as many steps as the output could have drawn, and no counter about the loop
//! can see it happen.
//!
//! That is what an elastic edge is exposed to and a smooth scroll is not. A glide covers a detent
//! in a couple of hundred milliseconds and moves several pixels every frame of it; a spring covers
//! the band in about a third of a second and *decelerates the whole way*, so the second half of its
//! return moves well under a device pixel per frame on a fast output. Composed onto the device grid
//! the return is then quantised: the edge stands still for a frame or two, jumps a pixel, stands
//! still again. Every frame runs, and half of them draw the picture the last one drew.
//!
//! So what is counted here is *composed positions that differ*, per refresh of the output, on three
//! outputs. It is the number the screen shows, and it is one.

mod support;

use std::time::Duration;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::Headless;
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// A list of rows in a scrollport a fraction of their height, with a known line height.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow: scroll; line-height: 20px }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
";

/// A window on an output refreshing `millihertz` times a second, holding one scrolling list.
fn listing(millihertz: u32) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    listing_on(millihertz, Headless::new())
}

/// The same on a desktop that answers something particular about scrolling.
fn listing_on(
    millihertz: u32,
    platform: Headless,
) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    let mut harness =
        support::app_with_text_on(platform, CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
            use zgui_view::{IntoView, View};
            let mut port = zgui_elements::column().class("port");
            for index in 0..200 {
                port = port.child(
                    zgui_elements::column()
                        .class("row")
                        .child(zgui_elements::text().child(format!("row {index}"))),
                );
            }
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(port)
                    .into_view()
                    .build(cx),
            )
        });
    harness.platform().offscreens()[0].set_refresh_rate_millihertz(Some(millihertz));
    harness.settle(8);
    harness
}

/// One frame of an output refreshing `millihertz` times a second.
fn interval(millihertz: u32) -> Duration {
    zgui_platform::refresh_interval(Some(millihertz))
}

/// Drags the list past its end by `pixels`, as a finger on a trackpad does.
///
/// A continuous phase rather than a detent, so that what the frames below carry is the spring and
/// nothing else: a detent installs a glide as well, and a run measuring both could not say which
/// of them moved the content.
fn drag_past_the_end(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    pixels: f32,
) {
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Touch,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
            delta: ScrollDelta::Pixels(zgui_geom::Size::new(CssPx(0.0), CssPx(pixels))),
            phase: ScrollPhase::Moved,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(4);
}

/// Where the scrolling list's contents are composed, as one comparable value.
fn composed(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> String {
    format!(
        "{:?}",
        harness
            .app()
            .windows()
            .first()
            .expect("a window")
            .scroll()
            .borrow()
            .composed()
    )
}

/// What one run of a spring produced.
struct Return {
    /// How many refreshes of the output the return lasted.
    refreshes: u64,
    /// How many frames the window ran over them.
    frames: u64,
    /// How many of those frames composed the content somewhere the last one had not.
    moves: u64,
}

/// Runs one whole return on a `millihertz` output and counts what it drew.
fn spring_back(millihertz: u32, pulled_by: f32) -> Return {
    let step = interval(millihertz);
    let mut harness = listing(millihertz);
    drag_past_the_end(&mut harness, pulled_by);
    assert!(
        harness
            .app()
            .windows()
            .first()
            .expect("a window")
            .scroll()
            .borrow()
            .is_animating(),
        "the drag left nothing springing back, so there is no return to measure"
    );

    harness.reset_counts();
    let mut held = Return {
        refreshes: 0,
        frames: 0,
        moves: 0,
    };
    let mut last = composed(&harness);
    while !harness
        .app()
        .windows()
        .first()
        .expect("a window")
        .scroll()
        .borrow()
        .settled()
        && held.refreshes < 600
    {
        harness.advance(step);
        held.frames += harness.pump();
        held.refreshes += 1;
        let now = composed(&harness);
        if now != last {
            held.moves += 1;
        }
        last = now;
    }
    harness.assert_park_invariant();
    held
}

#[test]
fn an_overscroll_spring_moves_the_content_once_per_refresh_on_every_output() {
    // The measurement the screen shows. A window on a fast output owes more frames per second than
    // one on a slow output and both owe one *visibly different* frame per refresh, so the same
    // return is expected to be drawn in four times as many steps at two hundred and forty hertz as
    // at sixty — and the ratio, which is what the eye reads, is one on all three.
    // Seventy-five hertz is in the list beside the two obvious ones because it is the rate whose
    // refresh interval is not a whole number of microseconds: a cadence held by rounding an
    // interval to something convenient is held at sixty and at two hundred and forty and lost
    // here.
    for millihertz in [240_000, 120_000, 75_000, 60_000] {
        let ran = spring_back(millihertz, -400.0);
        assert!(
            ran.refreshes > 20,
            "the return was over in {} refreshes on a {} hz output, which is too short to be a \
             cadence at all",
            ran.refreshes,
            millihertz / 1_000
        );
        assert_eq!(
            ran.frames,
            ran.refreshes,
            "the loop ran {} frames over {} refreshes of a {} hz output",
            ran.frames,
            ran.refreshes,
            millihertz / 1_000
        );
        assert_eq!(
            ran.moves,
            ran.refreshes,
            "the spring moved the content on {} of the {} frames it was given on a {} hz output — \
             a ratio of {:.2} per refresh. The frames all ran; the ones missing here drew the \
             picture the frame before them drew, which the renderer refuses as undamaged.",
            ran.moves,
            ran.refreshes,
            millihertz / 1_000,
            ran.moves as f64 / ran.refreshes as f64
        );
    }
}

#[test]
fn a_motion_that_begins_on_an_idle_window_does_not_spend_the_time_the_window_was_idle() {
    // A window with nothing to do parks, for as long as nothing happens to it. The frame that
    // starts a motion is the frame that drained the event which started it — so a motion advanced
    // against "the last frame that ran" spends the whole of that park in its first step. Four
    // seconds of it settles any spring and finishes any glide, inside the frame the wheel arrived
    // in, damaging nothing: the content simply appears at its destination, and the elastic edge is
    // drawn stretched on no frame at all.
    //
    // Every counter agrees with it, too. The motion was installed, it was advanced, it arrived,
    // and it is correctly no longer running.
    let millihertz = 240_000;
    let mut harness = listing(millihertz);
    harness.settle(8);
    assert!(
        !harness
            .app()
            .windows()
            .first()
            .expect("a window")
            .scroll()
            .borrow()
            .is_animating(),
        "the window is not idle to begin with, so the park below measures nothing"
    );

    // The park itself: the loop wakes for nothing over four seconds, exactly as a real one does.
    harness.run_for(Duration::from_secs(4), interval(millihertz) * 4);

    drag_past_the_end(&mut harness, -400.0);
    assert!(
        harness
            .app()
            .windows()
            .first()
            .expect("a window")
            .scroll()
            .borrow()
            .is_animating(),
        "the pull's whole return was spent in the frame it arrived in, so nothing was left to \
         animate and no frame of the bounce was ever drawn"
    );

    // And it is a whole return rather than a remnant of one: the spring still takes the frames it
    // would have taken on a window that had been busy all along.
    let mut refreshes = 0;
    while !harness
        .app()
        .windows()
        .first()
        .expect("a window")
        .scroll()
        .borrow()
        .settled()
        && refreshes < 600
    {
        harness.advance(interval(millihertz));
        harness.pump();
        refreshes += 1;
    }
    assert!(
        refreshes > 20,
        "the return was over in {refreshes} refreshes, which is not a return"
    );
}

#[test]
fn a_spring_that_has_come_back_leaves_the_content_on_the_device_grid() {
    // The bound on the other side. Composing a displacement off the grid is right while it is
    // moving and wrong once it has arrived: a list left half a pixel out is a list whose text is
    // resampled for as long as it is on the screen, which is the cost the snap exists to avoid.
    let mut harness = listing(240_000);
    let settled = topmost_row(&harness);
    drag_past_the_end(&mut harness, -400.0);
    assert!(
        topmost_row(&harness) > settled + 1.0,
        "the drag displaced nothing"
    );

    for _ in 0..600 {
        if harness
            .app()
            .windows()
            .first()
            .expect("a window")
            .scroll()
            .borrow()
            .settled()
        {
            break;
        }
        harness.advance(interval(240_000));
        harness.pump();
    }
    let arrived = topmost_row(&harness);
    assert_eq!(
        arrived, settled,
        "the return left the topmost row at {arrived} where a list that was never pulled has it \
         at {settled}"
    );
    assert_eq!(
        arrived.fract(),
        0.0,
        "the content came to rest off the device pixel grid"
    );
}

/// Where the highest row of the list is drawn, in device pixels down the surface.
///
/// Read out of the fragment tree rather than out of the offset, because where the content ends up
/// is what the claim is about: an offset is composed with a snap and a shift on the way to a
/// fragment, and only the fragment says where the pixels went.
fn topmost_row(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> f32 {
    let window = harness.app().windows().first().expect("a window");
    let layout = window.layout().borrow();
    let mut top: Option<f32> = None;
    for key in layout.keys() {
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            // A row, not a piece of the scrollport's own chrome. A thumb clamped to its minimum
            // length is exactly as tall as a row of this fixture and sits at the top of the
            // gutter, so a search by height alone measures the scrollbar and reports that nothing
            // moved however far the list went.
            if matches!(fragment.kind, zgui_layout::FragmentKind::Scrollbar { .. }) {
                continue;
            }
            if (fragment.border_box.size.height.0 - 20.0).abs() > 0.5 {
                continue;
            }
            top = Some(match top {
                Some(held) => held.min(fragment.border_box.origin.y.0),
                None => fragment.border_box.origin.y.0,
            });
        }
    }
    top.expect("the document has rows in it")
}

#[test]
fn a_detent_at_the_end_glides_and_springs_on_the_one_deadline() {
    // A wheel turned against the end of a list starts both motions at once: the glide carries what
    // the container could absorb and the spring carries what it could not. They are one window's
    // worth of movement and are owed one frame per refresh between them — a second deadline would
    // be a second frame per refresh, which is the pipeline run twice to present once.
    let millihertz = 240_000;
    // On a desktop that asks for the spring on a wheel, because that is the only input that starts
    // *both* motions from one event and this is about what the two of them together are owed. The
    // default refuses a detent the spring, which leaves a glide and nothing to share a deadline
    // with.
    let mut harness = listing_on(
        millihertz,
        Headless::new().with_scroll_settings(
            zgui_platform::scroll::ScrollSettings::desktop()
                .with_elastic(zgui_platform::scroll::Elastic::Always),
        ),
    );
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
            delta: ScrollDelta::Lines { x: 0.0, y: -20.0 },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(4);

    harness.reset_counts();
    let mut refreshes = 0;
    let mut frames = 0;
    let mut moves = 0;
    let mut last = composed(&harness);
    while harness
        .app()
        .windows()
        .first()
        .expect("a window")
        .scroll()
        .borrow()
        .is_animating()
        && refreshes < 600
    {
        harness.advance(interval(millihertz));
        frames += harness.pump();
        refreshes += 1;
        let now = composed(&harness);
        if now != last {
            moves += 1;
        }
        last = now;
    }

    assert!(refreshes > 20, "nothing was carried at all");
    assert_eq!(
        frames, refreshes,
        "the two motions asked for separate frames"
    );
    assert_eq!(
        moves,
        refreshes,
        "the content stood still for {} of the {refreshes} frames a glide and a spring were \
         running over",
        refreshes - moves
    );
    harness.assert_park_invariant();
}
