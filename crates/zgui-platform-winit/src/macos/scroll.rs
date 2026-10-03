//! The momentum phase of a scroll.
//!
//! AppKit reports two phases on a scroll event: the phase of the fingers on the trackpad, and the
//! momentum phase of the scroll the window server continues after they lift. winit folds the two
//! into one, so momentum arrives as a second gesture. A monitor here reads both phases of each
//! scroll event before winit receives it, and the translation of that event reads them back.

use core::cell::Cell;
use core::ptr::NonNull;
use std::sync::Once;

use block2::RcBlock;
use objc2::msg_send;
use objc2_app_kit::{NSEvent, NSEventMask, NSEventPhase};
use winit::event::{MouseScrollDelta, TouchPhase};
use zgui_vocab::ScrollPhase;

use crate::input::wheel;

/// The phases and deltas of one scroll event, as AppKit reported them.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Reading {
    /// The phase of the fingers.
    phase: NSEventPhase,
    /// The phase of the momentum after the fingers lift.
    momentum: NSEventPhase,
    /// The horizontal delta, in points.
    x: f64,
    /// The vertical delta, in points.
    y: f64,
}

thread_local! {
    /// The last scroll event AppKit dispatched on this thread.
    static LAST: Cell<Option<Reading>> = const { Cell::new(None) };
}

/// The largest difference between two deltas that are the same delta, in points.
///
/// winit converts the delta to physical pixels, and the conversion back is not exact.
const SAME_DELTA: f64 = 0.01;

/// Starts reading the phases of scroll events, once.
///
/// The monitor runs as AppKit receives a scroll event, before the view sees it, and returns the
/// event unchanged.
pub(crate) fn install() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let handler = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit hands the monitor a live event for the length of the call.
            let held = unsafe { event.as_ref() };
            // SAFETY: a scroll event answers `scrollingDeltaX` and `scrollingDeltaY` with a
            // `CGFloat`, which is an `f64` on every target AppKit supports.
            let (x, y): (f64, f64) = unsafe {
                (
                    msg_send![held, scrollingDeltaX],
                    msg_send![held, scrollingDeltaY],
                )
            };
            LAST.set(Some(Reading {
                phase: held.phase(),
                momentum: held.momentumPhase(),
                x,
                y,
            }));
            event.as_ptr()
        });
        // SAFETY: the handler returns the event it was given, as the monitor requires.
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::ScrollWheel,
                &handler,
            )
        };
        // The monitor stands for the life of the application.
        core::mem::forget(monitor);
    });
}

/// Where in a gesture a scroll that winit reported as `delta` in `phase` sits.
///
/// Reads the phases AppKit reported for the same event. A delta that does not match the last
/// reading, or a wheel that reports lines, keeps the phase winit reported.
pub(crate) fn phase(delta: MouseScrollDelta, phase: TouchPhase, scale_factor: f64) -> ScrollPhase {
    let fallback = wheel::phase(delta, phase);
    let MouseScrollDelta::PixelDelta(pixels) = delta else {
        return fallback;
    };
    let scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    let Some(reading) = LAST.get() else {
        return fallback;
    };
    let same = (pixels.x / scale - reading.x).abs() < SAME_DELTA
        && (pixels.y / scale - reading.y).abs() < SAME_DELTA;
    if !same {
        return fallback;
    }
    translate(reading.phase, reading.momentum).unwrap_or(fallback)
}

/// The phase of a scroll that AppKit reported with the finger phase `phase` and the momentum
/// phase `momentum`, or nothing when neither says.
fn translate(phase: NSEventPhase, momentum: NSEventPhase) -> Option<ScrollPhase> {
    if momentum.intersects(NSEventPhase::Ended | NSEventPhase::Cancelled) {
        return Some(ScrollPhase::Ended);
    }
    if momentum.intersects(NSEventPhase::Began | NSEventPhase::Changed | NSEventPhase::Stationary) {
        return Some(ScrollPhase::Momentum);
    }
    if phase.intersects(NSEventPhase::MayBegin | NSEventPhase::Began) {
        return Some(ScrollPhase::Started);
    }
    if phase.intersects(NSEventPhase::Changed | NSEventPhase::Stationary) {
        return Some(ScrollPhase::Moved);
    }
    if phase.intersects(NSEventPhase::Ended | NSEventPhase::Cancelled) {
        return Some(ScrollPhase::Ended);
    }
    None
}

#[cfg(test)]
mod tests {
    use objc2_app_kit::NSEventPhase;
    use winit::dpi::PhysicalPosition;
    use winit::event::{MouseScrollDelta, TouchPhase};
    use zgui_vocab::ScrollPhase;

    use super::{LAST, Reading, phase, translate};

    #[test]
    fn the_fingers_report_a_gesture_and_the_window_server_reports_momentum() {
        let none = NSEventPhase::None;
        assert_eq!(
            translate(NSEventPhase::MayBegin, none),
            Some(ScrollPhase::Started)
        );
        assert_eq!(
            translate(NSEventPhase::Began, none),
            Some(ScrollPhase::Started)
        );
        assert_eq!(
            translate(NSEventPhase::Changed, none),
            Some(ScrollPhase::Moved)
        );
        assert_eq!(
            translate(NSEventPhase::Ended, none),
            Some(ScrollPhase::Ended)
        );
        assert_eq!(
            translate(none, NSEventPhase::Began),
            Some(ScrollPhase::Momentum)
        );
        assert_eq!(
            translate(none, NSEventPhase::Changed),
            Some(ScrollPhase::Momentum)
        );
        assert_eq!(
            translate(none, NSEventPhase::Ended),
            Some(ScrollPhase::Ended)
        );
        assert_eq!(translate(none, none), None);
    }

    #[test]
    fn a_reading_of_another_event_is_not_used() {
        let delta = MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 20.0));
        LAST.set(Some(Reading {
            phase: NSEventPhase::None,
            momentum: NSEventPhase::Changed,
            x: 0.0,
            y: 10.0,
        }));
        assert_eq!(phase(delta, TouchPhase::Moved, 2.0), ScrollPhase::Momentum);
        assert_eq!(
            phase(delta, TouchPhase::Moved, 1.0),
            ScrollPhase::Moved,
            "a delta of 20 points is another event than the one read"
        );
        let lines = MouseScrollDelta::LineDelta(0.0, 1.0);
        assert_eq!(phase(lines, TouchPhase::Moved, 2.0), ScrollPhase::Discrete);
    }
}
