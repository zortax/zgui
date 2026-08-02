//! The two gestures, which are two different things wearing the same name.
//!
//! A **wheel** is discrete. One notch arrives, the framework turns it into a distance, and the list
//! glides there over a dozen or so frames that no further input takes part in. What it costs is
//! therefore a cost per *tick of the glide*, and the notch itself is a fifth of it.
//!
//! A **touchpad** is continuous and held. There is no glide to speak of while the fingers are down:
//! every frame carries a delta the user just produced, the phase says the gesture is still running,
//! and the framework may not clamp, may not decelerate and may not coalesce. What it costs is a cost
//! per *delivered delta*.
//!
//! Measuring only one of them measures half the scroll path. They are kept apart here rather than
//! averaged because the frames differ in kind, and a mean over two kinds of frame reports a frame
//! that does not exist.

use std::time::{Duration, Instant};

use zgui::geom::{Css, CssPx, Point, Size};
use zgui::runtime::Runtime;
use zgui::vocab::{
    Modifiers, PointerAction, PointerEvent, PointerId, PointerKind, ScrollDelta, ScrollPhase,
    Timestamp, WheelEvent,
};
use zgui_platform_headless::Harness;

/// One tick of a 120 Hz refresh, which is what a glide is advanced by.
const TICK: Duration = Duration::from_micros(8_333);

/// How many ticks of glide one wheel notch is carried for.
///
/// Enough that the list crosses several row boundaries, so the measurement contains both kinds of
/// frame a virtualised scroll has — the ones that only move the content, and the ones a row leaves
/// at one end and arrives at the other.
const GLIDE_TICKS: usize = 24;

/// How many deltas one touchpad gesture delivers between its `Started` and its `Ended`.
const TOUCH_STEPS: usize = 24;

/// How far one touchpad delta carries the list, in CSS pixels.
///
/// Larger than a row, so the same gesture crosses row boundaries at the same rate the wheel does
/// and the two are comparable in what they ask of the list rather than only in how they arrive.
const TOUCH_PIXELS: f32 = 30.0;

/// Where the pointer sits: the middle of the port, which is what a wheel event is aimed at.
///
/// Handed in rather than read back off the window, because the port height is the axis the glide
/// sweep varies and the caller is the one that chose it.
pub(crate) fn middle(height: f32) -> Point<CssPx, Css> {
    Point::new(CssPx(super::document::WIDTH / 2.0), CssPx(height / 2.0))
}

/// One wheel notch at `at`.
fn notch(at: Point<CssPx, Css>, lines: f32) -> zgui::platform::SurfaceEvent {
    zgui::platform::SurfaceEvent::Wheel {
        event: WheelEvent {
            delta: ScrollDelta::Lines { x: 0.0, y: lines },
            phase: ScrollPhase::Discrete,
            position: at,
            id: PointerId::MOUSE,
            kind: PointerKind::Mouse,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// One delta of a held gesture at `at`, in CSS pixels rather than notches.
fn held(at: Point<CssPx, Css>, pixels: f32, phase: ScrollPhase) -> zgui::platform::SurfaceEvent {
    zgui::platform::SurfaceEvent::Wheel {
        event: WheelEvent {
            delta: ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(pixels))),
            phase,
            position: at,
            id: PointerId::MOUSE,
            kind: PointerKind::Touch,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// Puts the pointer over the list, so a wheel event has somewhere to land.
pub(crate) fn aim(harness: &mut Harness<Runtime>, at: Point<CssPx, Css>) {
    harness.deliver_to_first(zgui::platform::SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(at),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(64);
}

/// Which way this pass scrolls, so a sweep never runs out of list and never runs out of top.
///
/// A gesture delivered where the list is already scrolled to is a gesture the scroller is entitled
/// to answer with nothing at all, and a measurement made of those measures the check that refuses
/// them.
fn direction(turn: usize) -> f32 {
    if (turn / 8).is_multiple_of(2) {
        1.0
    } else {
        -1.0
    }
}

/// One fast wheel: a notch, and the whole glide it starts.
fn wheel(harness: &mut Harness<Runtime>, at: Point<CssPx, Css>, turn: usize) -> Duration {
    let started = Instant::now();
    harness.deliver_to_first(notch(at, 6.0 * direction(turn)));
    harness.settle(64);
    for _ in 0..GLIDE_TICKS {
        harness.advance(TICK);
        harness.pump();
    }
    started.elapsed()
}

/// One touchpad gesture: started, twenty-four deltas at the refresh, ended.
fn touchpad(harness: &mut Harness<Runtime>, at: Point<CssPx, Css>, turn: usize) -> Duration {
    let pixels = TOUCH_PIXELS * direction(turn);
    let started = Instant::now();
    harness.deliver_to_first(held(at, pixels, ScrollPhase::Started));
    harness.settle(64);
    for _ in 0..TOUCH_STEPS {
        harness.deliver_to_first(held(at, pixels, ScrollPhase::Moved));
        harness.settle(64);
        harness.advance(TICK);
        harness.pump();
    }
    harness.deliver_to_first(held(at, 0.0, ScrollPhase::Ended));
    harness.settle(64);
    started.elapsed()
}

/// Which of the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Gesture {
    /// A discrete notch and the glide it starts.
    Wheel,
    /// A held gesture delivering a delta per frame.
    Touchpad,
}

impl Gesture {
    /// Both of them, in the order a report lists them.
    pub(crate) const ALL: [Self; 2] = [Self::Wheel, Self::Touchpad];

    /// What it is called in a printed line.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Wheel => "wheel",
            Self::Touchpad => "touchpad",
        }
    }

    /// Drives one pass of it.
    pub(crate) fn drive(
        self,
        harness: &mut Harness<Runtime>,
        at: Point<CssPx, Css>,
        turn: usize,
    ) -> Duration {
        match self {
            Self::Wheel => wheel(harness, at, turn),
            Self::Touchpad => touchpad(harness, at, turn),
        }
    }
}
