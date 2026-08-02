//! What a sequence of raw touches means.
//!
//! A finger produces presses, moves and releases and nothing else. Everything a touch interface is
//! made of — a tap, a long press, a drag, a flick that keeps going, a pinch — is a *reading* of that
//! stream, and it has to be produced somewhere because no platform in this backend's reach produces
//! it. The reading is here rather than in a component for the reason every other interpretation of
//! input is here: two components that each recognised a tap would disagree about what one is, and
//! the disagreement would show up as a control that needs to be pressed twice.
//!
//! # The rules the readings obey
//!
//! * **A tap is a press and a release that did not travel.** The slop is what makes a tap on a
//!   moving finger still a tap; without it a touch screen has no tap at all, because no finger is
//!   ever perfectly still.
//! * **A press that travels past the slop becomes a pan, and stops being a candidate for anything
//!   else.** That is what stops a list from activating the row a scroll started on.
//! * **A long press is a press that has neither travelled nor ended.** It is reported by the clock
//!   rather than by an event, so whoever owns the clock asks for it.
//! * **A pan reports the speed it ended with**, because a flick is a pan whose end had speed in it
//!   and there is nothing else in the stream that can say how fast a finger was moving.

pub mod longpress;
pub mod pan;
pub mod pinch;
pub mod tap;

use core::time::Duration;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use zgui_geom::{Css, CssPx, Point, Size};
use zgui_vocab::{PointerAction, PointerEvent, PointerId, PointerKind, Timestamp};

pub use crate::gesture::longpress::LONG_PRESS;
pub use crate::gesture::pan::{Track, Velocity};
pub use crate::gesture::tap::SLOP;

/// A reading of the touch stream.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Gesture {
    /// A finger touched and lifted without travelling.
    Tap {
        /// Which pointer.
        pointer: PointerId,
        /// Where it was.
        at: Point<CssPx, Css>,
    },
    /// A finger has been held still for long enough to mean something else.
    LongPress {
        /// Which pointer.
        pointer: PointerId,
        /// Where it is.
        at: Point<CssPx, Css>,
    },
    /// A finger has travelled past the slop and is now dragging.
    PanStart {
        /// Which pointer.
        pointer: PointerId,
        /// Where it started.
        from: Point<CssPx, Css>,
    },
    /// A dragging finger moved.
    PanMove {
        /// Which pointer.
        pointer: PointerId,
        /// How far it moved since the last reading.
        by: Size<CssPx, Css>,
    },
    /// A dragging finger lifted, with the speed it lifted at.
    PanEnd {
        /// Which pointer.
        pointer: PointerId,
        /// How fast it was travelling, in CSS pixels per second.
        velocity: Velocity,
    },
    /// Two fingers moved towards or away from each other.
    Pinch {
        /// How much the distance between them has changed since they went down.
        scale: f32,
    },
}

/// Reads gestures out of the raw pointer stream.
///
/// Only touch contacts are read. A mouse produces the same presses and moves, and reading a pan out
/// of a mouse drag would mean every text selection and every window-furniture drag also arrived as
/// a gesture — so the recogniser answers nothing for a mouse and a component that wants mouse drags
/// uses [`drag`](crate::drag), which is a different thing with different rules.
///
/// ```
/// use zgui_geom::{CssPx, Point};
/// use zgui_input::gesture::{Gesture, Gestures};
/// use core::time::Duration;
/// use zgui_vocab::{PointerAction, PointerEvent, PointerId, PointerKind, Timestamp};
///
/// let stamp = |millis: u64| Timestamp::from_origin(Duration::from_millis(millis));
///
/// let mut gestures = Gestures::default();
/// let finger = |x: f32, y: f32| PointerEvent {
///     id: PointerId::new(1),
///     kind: PointerKind::Touch,
///     primary: true,
///     position: Point::new(CssPx(x), CssPx(y)),
///     button: None,
///     pressure: None,
/// };
///
/// gestures.pointer(PointerAction::Pressed, &finger(10.0, 10.0), stamp(0));
/// let read = gestures.pointer(
///     PointerAction::Released,
///     &finger(11.0, 10.0),
///     stamp(80),
/// );
/// assert!(matches!(read.as_slice(), [Gesture::Tap { .. }]));
/// ```
#[derive(Clone, Debug, Default)]
pub struct Gestures {
    /// The contacts that are down, by pointer.
    contacts: FxHashMap<PointerId, Track>,
    /// The distance between the first two contacts when the second one went down.
    pinch_origin: Option<f32>,
}

impl Gestures {
    /// A recogniser with nothing in progress.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many contacts are down.
    pub fn contacts(&self) -> usize {
        self.contacts.len()
    }

    /// Whether any contact could still become something the clock has to report.
    ///
    /// This is what tells the loop whether to arm a deadline for a long press. With nothing held it
    /// is false, which is the state of every idle application.
    pub fn awaits_deadline(&self) -> bool {
        self.contacts
            .values()
            .any(|track| track.may_become_long_press())
    }

    /// Reads one pointer event, returning whatever it completed.
    pub fn pointer(
        &mut self,
        action: PointerAction,
        event: &PointerEvent,
        at: Timestamp,
    ) -> SmallVec<[Gesture; 2]> {
        let mut read = SmallVec::new();
        if event.kind != PointerKind::Touch {
            return read;
        }
        match action {
            PointerAction::Pressed => {
                self.contacts
                    .insert(event.id, Track::started(event.position, at));
                self.pinch_origin = self.spread();
            }
            PointerAction::Moved => {
                let Some(track) = self.contacts.get_mut(&event.id) else {
                    return read;
                };
                let moved = track.moved(event.position, at);
                if moved.began_panning {
                    read.push(Gesture::PanStart {
                        pointer: event.id,
                        from: track.origin(),
                    });
                }
                if track.is_panning() {
                    read.push(Gesture::PanMove {
                        pointer: event.id,
                        by: moved.by,
                    });
                }
                if let Some(scale) = self.pinch_scale() {
                    read.push(Gesture::Pinch { scale });
                }
            }
            PointerAction::Released => {
                let Some(track) = self.contacts.remove(&event.id) else {
                    return read;
                };
                if track.is_panning() {
                    read.push(Gesture::PanEnd {
                        pointer: event.id,
                        velocity: track.velocity(at),
                    });
                } else if !track.is_spent() {
                    read.push(Gesture::Tap {
                        pointer: event.id,
                        at: event.position,
                    });
                }
                self.pinch_origin = self.spread();
            }
            PointerAction::Cancelled => {
                // A cancelled contact produces no reading at all, which is the whole point of the
                // action existing: something else took the interaction over, and reporting a tap
                // for it would activate the control the gesture was taken away from.
                self.contacts.remove(&event.id);
                self.pinch_origin = self.spread();
            }
            // Nothing else in the stream contributes to a reading: an enter and a leave are about
            // where a pointer is, and a kind this build has never heard of is not guessed at.
            _ => {}
        }
        read
    }

    /// Reports every contact that has now been held long enough to be a long press.
    ///
    /// Driven by whoever owns the clock, because a long press is the one reading that is produced
    /// by time passing rather than by anything arriving.
    pub fn elapsed(&mut self, now: Timestamp) -> SmallVec<[Gesture; 2]> {
        let mut read = SmallVec::new();
        for (pointer, track) in &mut self.contacts {
            if track.becomes_long_press(now) {
                read.push(Gesture::LongPress {
                    pointer: *pointer,
                    at: track.at(),
                });
            }
        }
        read
    }

    /// When the earliest pending long press comes due.
    pub fn next_deadline(&self, now: Timestamp) -> Option<Duration> {
        self.contacts
            .values()
            .filter_map(|track| track.long_press_in(now))
            .min()
    }

    /// The distance between the first two contacts, if there are two.
    fn spread(&self) -> Option<f32> {
        let mut positions = self.contacts.values().map(|track| track.at());
        let first = positions.next()?;
        let second = positions.next()?;
        Some(pinch::distance(first, second))
    }

    /// How far the two contacts have spread since the second went down.
    fn pinch_scale(&self) -> Option<f32> {
        let origin = self.pinch_origin?;
        pinch::scale(origin, self.spread()?)
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Css, CssPx, Point};
    use zgui_vocab::{PointerAction, PointerEvent, PointerId, PointerKind, Timestamp};

    use super::{Gesture, Gestures};

    /// The instant `millis` after the application started.
    fn stamp(millis: u64) -> Timestamp {
        Timestamp::from_origin(core::time::Duration::from_millis(millis))
    }

    fn finger(id: u64, x: f32, y: f32) -> PointerEvent {
        PointerEvent {
            id: PointerId::new(id),
            kind: PointerKind::Touch,
            primary: id == 1,
            position: Point::<CssPx, Css>::new(CssPx(x), CssPx(y)),
            button: None,
            pressure: None,
        }
    }

    fn mouse(x: f32, y: f32) -> PointerEvent {
        PointerEvent::mouse(Point::new(CssPx(x), CssPx(y)))
    }

    #[test]
    fn a_press_and_a_release_that_travelled_is_a_pan_and_not_a_tap() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        let moved = gestures.pointer(PointerAction::Moved, &finger(1, 0.0, 40.0), stamp(20));
        assert!(matches!(moved.first(), Some(Gesture::PanStart { .. })));

        let ended = gestures.pointer(PointerAction::Released, &finger(1, 0.0, 40.0), stamp(30));
        assert!(
            matches!(ended.as_slice(), [Gesture::PanEnd { .. }]),
            "a scroll that ends over a row must not also activate it: {ended:?}"
        );
    }

    #[test]
    fn a_mouse_produces_no_gesture_at_all() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &mouse(0.0, 0.0), stamp(0));
        let read = gestures.pointer(PointerAction::Released, &mouse(0.0, 0.0), stamp(10));
        assert!(read.is_empty());
        assert_eq!(gestures.contacts(), 0);
    }

    #[test]
    fn a_cancelled_contact_produces_nothing() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        let read = gestures.pointer(PointerAction::Cancelled, &finger(1, 0.0, 0.0), stamp(10));
        assert!(read.is_empty(), "{read:?}");
        assert_eq!(gestures.contacts(), 0);
        assert!(!gestures.awaits_deadline());
    }

    #[test]
    fn two_fingers_moving_apart_are_a_pinch() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        gestures.pointer(PointerAction::Pressed, &finger(2, 100.0, 0.0), stamp(0));
        let read = gestures.pointer(PointerAction::Moved, &finger(2, 200.0, 0.0), stamp(20));
        let scale = read.iter().find_map(|gesture| match gesture {
            Gesture::Pinch { scale } => Some(*scale),
            _ => None,
        });
        assert_eq!(scale, Some(2.0));
    }

    #[test]
    fn one_finger_alone_is_never_a_pinch() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        let read = gestures.pointer(PointerAction::Moved, &finger(1, 60.0, 0.0), stamp(20));
        assert!(!read.iter().any(|g| matches!(g, Gesture::Pinch { .. })));
    }

    #[test]
    fn a_held_finger_becomes_a_long_press_once_and_not_again() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        assert!(gestures.awaits_deadline());
        assert!(gestures.elapsed(stamp(300)).is_empty());
        assert_eq!(gestures.elapsed(stamp(600)).len(), 1);
        assert!(
            gestures.elapsed(stamp(900)).is_empty(),
            "a held finger must fire one long press, not one per frame for as long as it is held"
        );
        assert!(!gestures.awaits_deadline());
    }

    #[test]
    fn a_long_press_that_has_already_fired_is_not_also_a_tap() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        gestures.elapsed(stamp(600));
        let read = gestures.pointer(PointerAction::Released, &finger(1, 0.0, 0.0), stamp(700));
        assert!(
            read.is_empty(),
            "lifting after a context menu has opened must not also activate the row: {read:?}"
        );
    }
}
