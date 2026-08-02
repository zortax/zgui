//! One contact, from touch-down to lift, and the speed it left with.

use core::time::Duration;

use zgui_geom::{Css, CssPx, Point, Size};
use zgui_vocab::Timestamp;

use crate::gesture::longpress::LONG_PRESS;
use crate::gesture::tap::travelled;

/// How fast a contact was travelling, in CSS pixels per second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Velocity {
    /// Sideways.
    pub x: f32,
    /// Downwards.
    pub y: f32,
}

/// What one move of a tracked contact produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Movement {
    /// How far it moved since the previous reading.
    pub by: Size<CssPx, Css>,
    /// Whether this is the move that turned it into a pan.
    pub began_panning: bool,
}

/// The window over which a lift's speed is measured.
///
/// Short enough that it reads the end of the gesture rather than its average — a finger that
/// dragged slowly and then flicked has flicked — and long enough to survive one stray sample.
const VELOCITY_WINDOW: Duration = Duration::from_millis(100);

/// One contact that is down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    /// Where it went down.
    origin: Point<CssPx, Css>,
    /// Where it is now.
    at: Point<CssPx, Css>,
    /// When it went down.
    started: Timestamp,
    /// Where it was at the start of the velocity window, and when that was.
    reference: (Point<CssPx, Css>, Timestamp),
    /// Whether it has travelled past the slop.
    panning: bool,
    /// Whether it has already produced something that rules a tap out.
    spent: bool,
}

impl Track {
    /// A contact that has just gone down at `at`.
    pub fn started(at: Point<CssPx, Css>, when: Timestamp) -> Self {
        Self {
            origin: at,
            at,
            started: when,
            reference: (at, when),
            panning: false,
            spent: false,
        }
    }

    /// Where it went down.
    pub fn origin(self) -> Point<CssPx, Css> {
        self.origin
    }

    /// Where it is now.
    pub fn at(self) -> Point<CssPx, Css> {
        self.at
    }

    /// Whether it is dragging.
    pub fn is_panning(self) -> bool {
        self.panning
    }

    /// Whether it has already meant something that a tap cannot follow.
    pub fn is_spent(self) -> bool {
        self.spent
    }

    /// Whether a long press is still possible for it.
    pub fn may_become_long_press(self) -> bool {
        !self.panning && !self.spent
    }

    /// Records that it moved to `to`.
    pub fn moved(&mut self, to: Point<CssPx, Css>, when: Timestamp) -> Movement {
        let by = Size::new(CssPx(to.x.0 - self.at.x.0), CssPx(to.y.0 - self.at.y.0));
        self.at = to;
        if when.saturating_since(self.reference.1) > VELOCITY_WINDOW {
            self.reference = (to, when);
        }
        let began_panning = !self.panning && travelled(self.origin, to);
        if began_panning {
            self.panning = true;
            self.spent = true;
        }
        Movement { by, began_panning }
    }

    /// Whether it has now been held long enough to be a long press, marking it spent if so.
    ///
    /// Answers true exactly once per contact: a held finger means "open the menu", not "open the
    /// menu again on every frame for as long as it is held".
    pub fn becomes_long_press(&mut self, now: Timestamp) -> bool {
        if !self.may_become_long_press() || now.saturating_since(self.started) < LONG_PRESS {
            return false;
        }
        self.spent = true;
        true
    }

    /// How long until it becomes a long press, if it still can.
    pub fn long_press_in(self, now: Timestamp) -> Option<Duration> {
        self.may_become_long_press()
            .then(|| LONG_PRESS.saturating_sub(now.saturating_since(self.started)))
    }

    /// How fast it is travelling, measured over the window ending now.
    pub fn velocity(self, now: Timestamp) -> Velocity {
        let elapsed = now.saturating_since(self.reference.1).as_secs_f32();
        if elapsed <= 0.0 {
            return Velocity::default();
        }
        Velocity {
            x: (self.at.x.0 - self.reference.0.x.0) / elapsed,
            y: (self.at.y.0 - self.reference.0.y.0) / elapsed,
        }
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{Css, CssPx, Point};
    use zgui_vocab::Timestamp;

    use super::{Track, Velocity};

    fn at(x: f32, y: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(y))
    }

    fn stamp(millis: u64) -> Timestamp {
        Timestamp::from_origin(Duration::from_millis(millis))
    }

    #[test]
    fn a_contact_becomes_a_pan_once_and_stays_one() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        assert!(!track.moved(at(0.0, 4.0), stamp(10)).began_panning);
        assert!(track.moved(at(0.0, 40.0), stamp(20)).began_panning);
        assert!(
            !track.moved(at(0.0, 80.0), stamp(30)).began_panning,
            "a pan begins once, or every move of a drag opens a new drag"
        );
        assert!(track.is_panning());
    }

    #[test]
    fn a_move_reports_the_step_and_not_the_position() {
        let mut track = Track::started(at(10.0, 10.0), stamp(0));
        let moved = track.moved(at(10.0, 50.0), stamp(16));
        assert_eq!(moved.by.height, CssPx(40.0));
        let again = track.moved(at(10.0, 60.0), stamp(32));
        assert_eq!(again.by.height, CssPx(10.0));
    }

    #[test]
    fn a_flick_at_the_end_of_a_slow_drag_reads_as_a_flick() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        // Half a second of slow dragging.
        for step in 1..=10 {
            track.moved(at(0.0, step as f32 * 5.0), stamp(step * 50));
        }
        // Then 60 pixels in 30 milliseconds.
        track.moved(at(0.0, 110.0), stamp(530));
        let velocity = track.velocity(stamp(530));
        let whole_gesture = 110.0 / 0.53;
        assert!(
            velocity.y > whole_gesture * 3.0,
            "the average over the whole gesture is {whole_gesture} px/s and the flick read {}; a \
             reading that averages the whole gesture cannot tell a flick from a slow drag that \
             ended in one",
            velocity.y
        );
    }

    #[test]
    fn a_contact_that_never_moved_has_no_speed() {
        let track = Track::started(at(0.0, 0.0), stamp(0));
        assert_eq!(track.velocity(stamp(0)), Velocity::default());
        assert_eq!(track.velocity(stamp(200)), Velocity::default());
    }

    #[test]
    fn a_panning_contact_can_no_longer_become_a_long_press() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        track.moved(at(0.0, 40.0), stamp(20));
        assert!(!track.may_become_long_press());
        assert!(!track.becomes_long_press(stamp(900)));
        assert_eq!(track.long_press_in(stamp(0)), None);
    }
}
