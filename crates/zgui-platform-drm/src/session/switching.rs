//! Whether this session is waiting to hear that a terminal it asked for has moved.
//!
//! Every cursor plane goes back before the ask, and it stays back until the daemon says what
//! happened. **The ask is the last moment a session that is leaving still holds DRM master**, so it
//! is the last moment a plane can be cleared: whatever is on a cursor plane stays there until
//! somebody names that plane in a commit, and a session that draws its own pointer into the frame
//! names it never. [`Session::switch`](super::Session::switch) answers as soon as the request goes
//! out, the terminal moves a turn or more later, and the loop's own cursor commit runs further down
//! the same turn. So a plane taken again straight away is a plane the next session inherits this
//! program's pointer on. [`crate::cursor`] states the rest.
//!
//! # An ask that moves no terminal
//!
//! The daemon can take the request and change nothing. Asking for the terminal this run is already
//! on is how a person meets it, and it is one of several ways: a daemon that accepts the request and
//! then refuses it, a terminal that exists with no session on it, and a daemon restarted while the
//! request is in flight all end the same way. Neither a suspend nor a resume arrives, so a session
//! that waited for one of them would wait for the rest of the run, and that display would carry no
//! pointer for as long as the program ran.
//!
//! # The bound on that wait
//!
//! The ask records the moment it is due an answer at, every turn reads the clock against it, and a
//! session that still holds the screen when that moment passes takes its planes back. One bound
//! covers every reason a switch can fail to happen, including the reasons nobody has thought of,
//! and it asks the machine nothing: no terminal number, no `/sys/class/tty/tty0/active`, and no
//! comparison between the two.
//!
//! **It cannot misfire on the switch that does happen.** A session whose terminal moved is
//! suspended by then, so `Presence::is_active` is false and the loop commits nothing. The suspend
//! and the resume each settle the record where they arrive, so nothing is left to expire.

use std::time::{Duration, Instant};

use crate::session::presence::{Presence, Transition};

/// How long a session waits to hear that a terminal it asked for has moved.
///
/// The spike this backend's session was written from measured the disable arriving **16 ms** after
/// the ask, from logind on one ordinary machine. This bound is thirty times that measurement.
///
/// It is generous because the two ways of being wrong cost different things. A bound shorter than
/// the daemon's answer puts the pointer back on a plane the next session is about to inherit, which
/// is what the give-back exists to prevent; and the commit that puts it there runs after logind has
/// dropped DRM master, so the kernel refuses it and
/// [`Cursor::commit`](crate::cursor::Cursor::commit) takes that display off its cursor plane for
/// the rest of the run. A bound far beyond the daemon's answer costs half a second with no pointer,
/// once, after an ask that moved no terminal.
pub(crate) const DISABLE_WITHIN: Duration = Duration::from_millis(500);

/// What a session that asked for a terminal is waiting for.
///
/// One of these belongs to a running loop, beside [`Presence`]. The ask records the moment an answer
/// is due at, and every turn reads that moment against the clock. The answer is whether the cursor
/// planes come back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Switching {
    /// The moment the daemon's answer is due at, while this session waits for one.
    due: Option<Instant>,
}

impl Switching {
    /// Returns a session that has asked for no terminal.
    pub(crate) const fn nothing() -> Self {
        Self { due: None }
    }

    /// Records that the ask went out, as of `now`.
    ///
    /// The moment is the loop's own reading of its clock, so a test moves it by hand. The request
    /// itself goes out microseconds later, which is nothing against [`DISABLE_WITHIN`].
    pub(crate) fn asked(&mut self, now: Instant) {
        self.due = Some(now + DISABLE_WITHIN);
    }

    /// Returns the moment this session is due an answer at, while it waits for one.
    ///
    /// The frame loop waits no longer than this. Nothing on a console wakes a parked loop for a
    /// moment of its own, and a console nobody is typing on parks until something happens. A bound
    /// nothing waited on would be read at the next key or the next frame, and the pointer would
    /// stay off a still screen until a person moved something.
    pub(crate) const fn due(self) -> Option<Instant> {
        self.due
    }

    /// Reads what the daemon said and what the clock reads, and answers whether the cursor planes
    /// come back.
    ///
    /// Called once a turn, after `Presence::turn`, and given what that answered.
    ///
    /// **A suspend and a resume both settle the ask.** The terminal moved, so the planes are the
    /// business of the session that has the screen, and the resume takes them back through
    /// [`Cursor::forget_the_plane`](crate::cursor::Cursor::forget_the_plane). A record left to
    /// expire over one of them would ask for the planes a second time, on a display that has
    /// already been put right.
    ///
    /// **A session that is away answers nothing.** It holds no DRM master and commits nothing, so
    /// there is nothing there to take back, and the resume settles the record when it arrives.
    pub(crate) fn turn(&mut self, moved: Option<Transition>, held: Presence, now: Instant) -> bool {
        if moved.is_some() {
            self.due = None;
            return false;
        }
        let overdue = held.is_active() && self.due.is_some_and(|due| now >= due);
        if overdue {
            self.due = None;
        }
        overdue
    }
}

#[cfg(test)]
mod tests {
    //! Every way an ask ends, over a clock written here.
    //!
    //! No card, no daemon and no terminal. What the loop does with the answer is one call on
    //! [`Planes`](crate::cursor::Planes), and what that call does to a plane is
    //! [`Cursor`](crate::cursor::Cursor)'s own and covered where it lives.

    use std::time::Duration;

    use super::{DISABLE_WITHIN, Instant, Switching};
    use crate::session::presence::{Presence, Transition};

    /// Returns a session that is away, which is a suspend already read.
    fn away() -> Presence {
        let mut presence = Presence::holding();
        assert_eq!(
            presence.turn(&[zgui_seat::Change::Disabled]),
            Some(Transition::Suspend)
        );
        presence
    }

    #[test]
    fn a_turn_where_nobody_asked_for_a_terminal_takes_nothing_back() {
        // The ordinary turn, which is every turn of every run that never presses the chord. A
        // cursor put back on a plane nobody gave back would redraw a frame for nothing on the
        // fallback path, and the answer has to be false whatever the clock reads.
        let mut switching = Switching::nothing();
        let now = Instant::now();

        assert!(!switching.turn(None, Presence::holding(), now));
        assert!(!switching.turn(None, Presence::holding(), now + Duration::from_secs(3_600)));
        assert_eq!(switching.due(), None, "and nothing is waited for");
    }

    #[test]
    fn an_ask_is_due_an_answer_a_bounded_time_later() {
        let mut switching = Switching::nothing();
        let now = Instant::now();

        switching.asked(now);

        assert_eq!(
            switching.due(),
            Some(now + DISABLE_WITHIN),
            "the moment the loop waits until, taken from the clock it was handed"
        );
    }

    #[test]
    fn a_disable_inside_the_bound_leaves_the_planes_where_they_are() {
        // The switch that happened. The daemon reported the terminal moving, so the planes are the
        // next session's business — and the record goes with it, so nothing expires afterwards and
        // asks for a plane this session no longer holds.
        let mut switching = Switching::nothing();
        let now = Instant::now();
        switching.asked(now);

        let read = now + Duration::from_millis(16);
        assert!(
            !switching.turn(Some(Transition::Suspend), away(), read),
            "the suspend does the rest"
        );
        assert_eq!(switching.due(), None, "and the ask is settled");
        assert!(
            !switching.turn(None, away(), now + Duration::from_secs(60)),
            "so no later turn takes a plane back"
        );
    }

    #[test]
    fn an_ask_nothing_answered_takes_the_planes_back_when_the_bound_passes() {
        // The defect this bound closes. The daemon took the request and moved no terminal, so
        // neither a suspend nor a resume is ever coming: a session that waited for one would draw
        // no pointer on that display for the rest of the run.
        let mut switching = Switching::nothing();
        let now = Instant::now();
        switching.asked(now);

        assert!(
            !switching.turn(None, Presence::holding(), now + Duration::from_millis(16)),
            "the daemon's own answer arrives around here, so nothing is decided yet"
        );
        assert!(
            !switching.turn(
                None,
                Presence::holding(),
                now + DISABLE_WITHIN - Duration::from_millis(1)
            ),
            "and a turn a millisecond before the bound still waits"
        );

        assert!(switching.turn(None, Presence::holding(), now + DISABLE_WITHIN));

        assert_eq!(switching.due(), None);
        assert!(
            !switching.turn(None, Presence::holding(), now + Duration::from_secs(60)),
            "the planes come back once, and the loop asks the driver for nothing every turn after \
             it"
        );
    }

    #[test]
    fn a_disable_and_a_resume_leave_nothing_to_expire() {
        // A person who switched away and came back. Both edges settle the ask, and it is the
        // resume's own forgetting that plans the next commit — so a record still standing here
        // would take a plane back that the resume has already written.
        let mut switching = Switching::nothing();
        let now = Instant::now();
        switching.asked(now);
        let mut presence = Presence::holding();

        assert!(!switching.turn(presence.turn(&[zgui_seat::Change::Disabled]), presence, now));
        assert!(!switching.turn(
            presence.turn(&[zgui_seat::Change::Enabled]),
            presence,
            now + Duration::from_secs(4)
        ));

        assert!(presence.is_active(), "the screen is this session's again");
        assert_eq!(switching.due(), None);
        assert!(
            !switching.turn(None, presence, now + Duration::from_secs(60)),
            "and nothing is left to take back"
        );
    }

    #[test]
    fn a_session_that_is_away_takes_nothing_back() {
        // It holds no DRM master, so a commit there is refused — which takes the display off its
        // cursor plane for the rest of the program. The resume is what puts such a session right.
        let mut switching = Switching::nothing();
        let now = Instant::now();
        switching.asked(now);

        assert!(!switching.turn(None, away(), now + DISABLE_WITHIN));
        assert_eq!(
            switching.due(),
            Some(now + DISABLE_WITHIN),
            "and the record waits for the resume that settles it"
        );
    }
}
