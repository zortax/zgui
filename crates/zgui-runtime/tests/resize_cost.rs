//! What a drag costs, counted in layouts rather than in frames.
//!
//! A frame is not the unit the complaint is about. The complaint is that a window on a slow output
//! shows sizes it has already left behind, and what produces those sizes is the *pipeline*: a
//! layout, a full-surface repaint and a swapchain rebuild, once per configure, at the rate a
//! compositor delivers configures rather than the rate an output can show them. So the quantity
//! asserted on here is the number of times the document was laid out into the viewport, taken from
//! the layout engine's own counter, and the claim is that it is bounded by *elapsed time over the
//! output's refresh interval* and by nothing else.
//!
//! That bound is refresh-rate independent as a *statement* — it holds at seventy-five hertz and at
//! two hundred and forty — which is exactly why it can be settled with no display server at all.
//! What differs between the two outputs is only the number the bound evaluates to, and the last
//! test here is what stops the bound from being satisfied by a window that ignores its output
//! entirely.
//!
//! # Why the counter and not the frame count
//!
//! A loop can decline a frame in two ways that look identical from outside and are not: by never
//! being offered one, and by being offered one and running the whole pipeline in it. The platform
//! counts what it offered; only the layout counter can say what the window actually did with it.
//! Every measurement below is a difference on that counter across the drag, taken under a lock,
//! because the counters are one set of process-wide atomics and this binary's tests run in
//! parallel.

mod support;

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use zgui_geom::{Device, DevicePx, Size};
use zgui_platform::SurfaceEvent;
use zgui_profile::{Counter, counter};
use zgui_view::{BuildCx, IntoView, View};

/// A root that is exactly as large as whatever contains it, so its fragment is the surface.
const CSS: &str = "root { display: block; width: 100%; height: 100% }";

/// The height every configure in this file carries, so only the width identifies a step.
const HEIGHT: f32 = 600.0;

/// The width the first configure of a drag moves the window to.
const FIRST_WIDTH: f32 = 800.0;

/// How long each drag below lasts, in milliseconds and in turns of the loop.
const DRAG_MILLIS: u64 = 120;

/// Held for the length of one measurement, because the layout counter is process-wide.
///
/// Without it two tests measuring at once each read the other's layouts and both bounds pass for
/// the wrong reason — which is the shape of a budget that measures nothing.
static MEASURING: Mutex<()> = Mutex::new(());

/// Takes the measuring lock, ignoring a poisoning left behind by a test that already failed.
///
/// A failed assertion inside one measurement would otherwise poison the lock and turn every other
/// test in this file into the same panic, hiding whether they would have passed — which is the one
/// thing a suite reporting a regression most needs to say.
fn measuring() -> MutexGuard<'static, ()> {
    MEASURING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What one drag cost.
#[derive(Debug)]
struct Cost {
    /// How many times the document was laid out into the viewport.
    layouts: u64,
    /// How many times the renderer was pointed at a new surface extent.
    rebuilds: u64,
    /// How many offered frames the window refused.
    declined: u64,
    /// How many layouts were checked against the size the window was at that moment.
    checked: u64,
}

/// How a drag is delivered: how many configures per turn, and what else arrives beside them.
#[derive(Clone, Copy)]
struct Drag {
    /// How fast the output the window is on refreshes, or nothing if it does not say.
    output: Option<u32>,
    /// How many configures arrive in each turn of the loop.
    per_turn: u64,
    /// Whether a pointer sample arrives beside every configure.
    pointer: bool,
}

impl Drag {
    /// A drag at one configure per turn on an output refreshing at `millihertz`.
    const fn on(output: Option<u32>) -> Self {
        Self {
            output,
            per_turn: 1,
            pointer: false,
        }
    }

    /// The same drag with `per_turn` configures in every turn instead of one.
    const fn at(self, per_turn: u64) -> Self {
        Self { per_turn, ..self }
    }

    /// The same drag with a pointer sample beside every configure.
    ///
    /// This is not an embellishment. A compositor that is resizing a window is also moving a
    /// pointer, a window being resized is usually being hovered, and a document being resized
    /// often has something running in it — so a drag with nothing else happening is the one drag
    /// that never occurs.
    const fn with_a_pointer(self) -> Self {
        Self {
            pointer: true,
            ..self
        }
    }
}

/// The application under test: one element that fills the window.
fn app() -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app(CSS, |cx: &mut BuildCx<'_>| {
        Box::new(zgui_elements::column().class("root").into_view().build(cx))
    })
}

/// A surface extent of `width` by [`HEIGHT`].
fn wide(width: f32) -> Size<DevicePx, Device> {
    Size::new(DevicePx(width), DevicePx(HEIGHT))
}

/// The surface every event is delivered to.
fn only_surface(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> zgui_platform::SurfaceId {
    harness
        .platform()
        .offscreens()
        .first()
        .map(|surface| zgui_platform::Surface::id(surface.as_ref()))
        .expect("the application opened its window")
}

/// The extent of the root box's fragment: what the last layout was actually built for.
fn laid_out(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> Size<DevicePx, Device> {
    let window = &harness.app().windows()[0];
    let layout = window.layout().borrow();
    let root = layout.root().expect("the document has a root box");
    let fragment = *layout
        .fragments_of_box(root)
        .first()
        .expect("the root box produced a fragment");
    layout
        .fragment(fragment)
        .expect("the fragment is live")
        .border_box
        .size
}

/// A pointer sample, as one arrives beside a configure during a real drag.
fn pointer_at(x: f32, at: Duration) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action: zgui_vocab::PointerAction::Moved,
        event: zgui_vocab::PointerEvent::mouse(zgui_geom::Point::new(
            zgui_geom::CssPx(x),
            zgui_geom::CssPx(10.0),
        )),
        modifiers: zgui_vocab::Modifiers::default(),
        timestamp: zgui_vocab::Timestamp::from_origin(at),
    }
}

/// Runs one drag of [`DRAG_MILLIS`] turns and reports what it cost.
///
/// Each turn delivers `per_turn` configures — a window system hands over everything that arrived
/// while the last frame was being drawn, all in one turn — gives the loop one chance to draw, and
/// then moves the clock by a millisecond. Every configure also sets the surface's redraw flag, as a
/// windowing backend does on its own account, so the question the loop is being asked is not "did
/// it request fewer frames" but "did it *do* less in the frames it was handed".
///
/// The clock is held, so the turns belong to this function rather than to the harness.
fn cost_of(drag: Drag, _guard: &MutexGuard<'_, ()>) -> Cost {
    let mut harness = app();
    let surface = only_surface(&harness);
    if let Some(millihertz) = drag.output {
        harness
            .platform()
            .offscreens()
            .first()
            .expect("the application opened its window")
            .set_refresh_rate_millihertz(Some(millihertz));
    }
    harness.settle(8);
    harness.hold_clock(true);
    harness.redraw_on_configure(true);
    // Far enough from the window's own first frame that the drag's first configure is answered
    // where it arrives, exactly as the first sample of a real drag is.
    harness.advance(Duration::from_millis(100));
    harness.reset_counts();

    let layouts_before = counter::get(Counter::LayoutReachedRoot);
    let rebuilds_before = harness.app().windows()[0].surface_configures();
    let declined_before = harness.app().windows()[0].declined_frames();
    let mut checked = 0;
    let mut width = FIRST_WIDTH;

    for turn in 0..DRAG_MILLIS {
        let at = Duration::from_millis(100 + turn);
        let mut batch = Vec::new();
        for _ in 0..drag.per_turn {
            batch.push(SurfaceEvent::Resized(wide(width)));
            if drag.pointer {
                batch.push(pointer_at(width - FIRST_WIDTH, at));
            }
            width += 1.0;
        }
        harness.deliver_all(surface, batch);
        let before = counter::get(Counter::LayoutReachedRoot);
        harness.pump();
        if counter::get(Counter::LayoutReachedRoot) > before {
            checked += 1;
            // The whole of the user-visible complaint in one assertion: a loop that drains a queue
            // of configures instead of sampling the level lays out every intermediate size one
            // refresh apart, so the content is seen to arrive after the drag that produced it has
            // finished.
            assert_eq!(
                laid_out(&harness),
                wide(width - 1.0),
                "a layout during the drag was built for a size the window had already left"
            );
        }
        harness.advance(Duration::from_millis(1));
    }

    let window = &harness.app().windows()[0];
    let cost = Cost {
        layouts: counter::get(Counter::LayoutReachedRoot) - layouts_before,
        rebuilds: window.surface_configures() - rebuilds_before,
        declined: window.declined_frames() - declined_before,
        checked,
    };
    harness.assert_park_invariant();
    harness.shut_down();
    cost
}

/// The most layouts a drag of [`DRAG_MILLIS`] could ever be seen to make on `output`.
///
/// One per frame of the output, plus the one the drag's first configure is answered by where it
/// arrives.
fn ceiling(output: Option<u32>) -> u64 {
    let interval = zgui_platform::refresh_interval(output).as_secs_f64();
    (DRAG_MILLIS as f64 / 1_000.0 / interval).ceil() as u64 + 1
}

/// How finely the wait for the window to catch up is observed.
///
/// The clock only moves when this test moves it, so this is the resolution of every settling
/// figure below and the amount by which one can overshoot the truth. It is named rather than
/// written into the assertion so that the bound states its own precision instead of quietly
/// absorbing it.
const SETTLING_STEP: Duration = Duration::from_micros(250);

/// What the window settled at, and how long after the last configure it got there.
#[derive(Debug)]
struct Settling {
    /// The extent the last configure of the burst asked for.
    asked: Size<DevicePx, Device>,
    /// What the window had been laid out for at the moment the burst ended.
    when_the_burst_ended: Size<DevicePx, Device>,
    /// How long after the last configure the window was laid out for `asked`.
    took: Duration,
}

/// Delivers a burst of configures that must be deferred, then waits for the window to catch up.
///
/// The burst is delivered in a single turn, immediately after a resize frame has run, so that
/// every configure in it lands inside that frame's own refresh interval and cannot be answered
/// where it arrives. That is what makes the wait afterwards mean something: the window is
/// definitely behind when the last configure stops arriving, and the only thing that can bring
/// it up to date is the deadline the refusal left behind.
fn settling_of(output: Option<u32>, _guard: &MutexGuard<'_, ()>) -> Settling {
    let mut harness = app();
    let surface = only_surface(&harness);
    if let Some(millihertz) = output {
        harness
            .platform()
            .offscreens()
            .first()
            .expect("the application opened its window")
            .set_refresh_rate_millihertz(Some(millihertz));
    }
    harness.settle(8);
    harness.hold_clock(true);
    harness.redraw_on_configure(true);
    harness.advance(Duration::from_millis(100));

    // One resize frame, which is what everything after it is paced against.
    harness.deliver(surface, SurfaceEvent::Resized(wide(FIRST_WIDTH)));
    harness.settle(8);

    // The rest of the drag, all of it inside that frame's interval and none of it answered.
    let widths: Vec<f32> = (1..=16)
        .map(|step| FIRST_WIDTH + step as f32 * 7.0)
        .collect();
    harness.deliver_all(
        surface,
        widths
            .iter()
            .map(|width| SurfaceEvent::Resized(wide(*width))),
    );
    harness.pump();
    let asked = wide(*widths.last().expect("the burst is not empty"));
    let when_the_burst_ended = laid_out(&harness);

    // Nothing else arrives from here. Only the deadline the refusals left behind can move it.
    let step = SETTLING_STEP;
    let mut took = Duration::ZERO;
    while laid_out(&harness) != asked && took < Duration::from_millis(200) {
        harness.advance(step);
        harness.pump();
        took += step;
    }
    harness.assert_park_invariant();
    harness.shut_down();
    Settling {
        asked,
        when_the_burst_ended,
        took,
    }
}

#[test]
fn a_drag_that_stops_is_on_the_screen_within_one_frame_of_the_output() {
    // The whole of the user's complaint, stated as the thing they would check: let go of the
    // corner, and the window is the size you let go at — not the size it was two frames ago,
    // and not a size it reaches only because something unrelated later asked for a frame.
    //
    // The bound is one refresh interval of the output the window is on, and it is asserted at
    // three different rates so that it cannot be satisfied by a constant.
    let guard = measuring();
    for output in [Some(60_000), Some(75_000), Some(240_000)] {
        let interval = zgui_platform::refresh_interval(output);
        let settled = settling_of(output, &guard);

        // Without this the wait below measures nothing: a window that was already up to date
        // when the burst ended converges in zero time on any implementation whatsoever,
        // including one that never defers a configure at all.
        assert_ne!(
            settled.when_the_burst_ended, settled.asked,
            "at {output:?} mHz the burst was answered where it arrived, so nothing was deferred \
             and the convergence measured below is vacuous"
        );
        // One refresh interval, plus the one step of clock this test observes at — the window
        // can be found up to date only at the first step after it became so.
        assert!(
            settled.took <= interval + SETTLING_STEP,
            "at {output:?} mHz the window took {:?} to show the size the drag ended at, against \
             a refresh interval of {interval:?} observed {SETTLING_STEP:?} at a time",
            settled.took
        );
        assert!(
            !settled.took.is_zero(),
            "at {output:?} mHz the window caught up without the clock moving at all, which means \
             the burst was never deferred"
        );
    }
}

#[test]
fn quadrupling_the_configures_over_the_same_span_of_time_buys_no_extra_layouts() {
    // The property the whole design rests on, stated so that only the design can satisfy it: what
    // bounds the work is the time the drag took and the output it happened on, never the number of
    // configures that arrived inside it. A loop that drains a queue answers this by laying out four
    // times as often; a loop that samples a level answers it with the same number twice.
    let guard = measuring();
    let once = cost_of(Drag::on(Some(75_000)), &guard);
    let four_times = cost_of(Drag::on(Some(75_000)).at(4), &guard);

    assert_eq!(
        four_times.layouts,
        once.layouts,
        "{} configures over {DRAG_MILLIS}ms cost {} layouts where {} configures over the same span \
         cost {}; the work is being bounded by how many arrived rather than by how many could be \
         seen",
        DRAG_MILLIS * 4,
        four_times.layouts,
        DRAG_MILLIS,
        once.layouts
    );
    assert!(
        once.layouts <= ceiling(Some(75_000)),
        "a {DRAG_MILLIS}ms drag laid out {} times at 75 Hz; {} is everything that could have been \
         seen",
        once.layouts,
        ceiling(Some(75_000))
    );
    assert!(once.layouts > 0, "the resize was never drawn at all");
    // Without this the bound above is satisfied by a window that laid out once and then stopped:
    // every turn's newest-size check is skipped, and the assertion inside the drag proves nothing.
    assert_eq!(
        once.checked, once.layouts,
        "the newest-size check ran on {} of {} layouts",
        once.checked, once.layouts
    );
}

#[test]
fn a_swapchain_is_rebuilt_once_per_layout_and_never_once_per_configure() {
    // The expensive half, counted separately because it is the one that does not shrink when the
    // step is small: rebuilding a swap chain waits for the graphics device to go completely idle,
    // so it costs the same whether the window moved by a pixel or across a monitor.
    let guard = measuring();
    let cost = cost_of(Drag::on(Some(75_000)).at(4), &guard);

    assert_eq!(
        cost.rebuilds, cost.layouts,
        "the drag laid out {} times and rebuilt the swapchain {} times",
        cost.layouts, cost.rebuilds
    );
    assert!(
        cost.rebuilds <= ceiling(Some(75_000)),
        "{} configures rebuilt the swapchain {} times, against a ceiling of {}",
        DRAG_MILLIS * 4,
        cost.rebuilds,
        ceiling(Some(75_000))
    );
}

#[test]
fn a_pointer_moving_beside_the_drag_does_not_undo_the_pacing() {
    // The defect this file was written for, and the one that made the whole design conditional. A
    // window that paces only the configures still runs the entire pipeline for any frame that
    // something *else* asked for — and while a window is being resized, something else always is.
    // The frame is happening anyway, so it reconfigures on its way through, and the swapchain is
    // rebuilt once per pointer sample: the rate the compositor delivers, not the rate the output
    // can show.
    let guard = measuring();
    let alone = cost_of(Drag::on(Some(75_000)), &guard);
    let hovered = cost_of(Drag::on(Some(75_000)).with_a_pointer(), &guard);

    assert_eq!(
        hovered.layouts, alone.layouts,
        "a drag with a pointer sample beside every configure cost {} layouts where the same drag \
         alone cost {}; the pacing is being undone by whatever else happens to want a frame",
        hovered.layouts, alone.layouts
    );
    assert_eq!(
        hovered.rebuilds, alone.rebuilds,
        "the same drag rebuilt the swapchain {} times with a pointer moving and {} without",
        hovered.rebuilds, alone.rebuilds
    );
    assert!(
        hovered.declined > 0,
        "the window refused no frame at all, so nothing here was ever tested: a drag that is never \
         offered a frame it does not want cannot show that it would decline one"
    );
}

#[test]
fn the_same_drag_costs_more_on_a_faster_output_and_less_on_a_slower_one() {
    // What stops every bound above from being satisfied by a window that reads no output at all.
    // A constant sixty-hertz assumption passes all of them while capping a two-hundred-and-forty
    // hertz display at a quarter of the resize frames it can show, and holding a seventy-five hertz
    // one to frames it cannot.
    let guard = measuring();
    let slow = cost_of(Drag::on(Some(75_000)).at(4), &guard);
    let fast = cost_of(Drag::on(Some(240_000)).at(4), &guard);

    assert!(
        fast.layouts > slow.layouts * 2,
        "the same drag laid out {} times at 75 Hz and {} at 240 Hz; the two outputs are more than \
         three refresh intervals apart, so a window reading its own surface cannot answer them at \
         nearly the same rate",
        slow.layouts,
        fast.layouts
    );
    assert!(
        fast.layouts <= ceiling(Some(240_000)),
        "a {DRAG_MILLIS}ms drag laid out {} times at 240 Hz, above the {} that could be seen",
        fast.layouts,
        ceiling(Some(240_000))
    );
    assert!(slow.layouts > 0 && fast.layouts > 0, "nothing was drawn");
}
