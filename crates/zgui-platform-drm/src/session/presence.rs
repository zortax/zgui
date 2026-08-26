//! Whether this session holds the screen, and what a turn of the loop has to do about it.
//!
//! A seat reports what happened to it as a queue of [`Change`]s, and a loop reads that queue once
//! per turn. Out of that queue the loop needs one of three answers: give the devices back, take
//! them again, or carry on. Working that out is arithmetic over a list, so it is written here,
//! apart from the device, the descriptor and the frame loop that carries it out.
//!
//! # What a queue holds
//!
//! One turn can carry any number of changes, because a person can switch terminal twice while a
//! loop is inside one wait. What matters is where the queue leaves the session, and whether the
//! devices moved on the way. **A switch away and a switch back can both be read in one turn**, and
//! that pair is one thing to do.
//!
//! Four rules cover every queue, and each one is a way a loop that folded the queue naively would
//! be wrong:
//!
//! * **Two enables in a row are one resume.** Reopening the input devices twice would leave the
//!   daemon holding a record this run has no way to give back.
//! * **A disable and an enable inside one turn are one resume.** There is no window between them:
//!   by the time the loop reads either, the terminal has already moved twice. The seat still holds
//!   every device on that path, and the kernel has revoked all of them.
//! * **An enable while the session is already active is nothing at all.** A resume there would
//!   close every input device to open it again, and put a mode back that is already up.
//! * **A disable while the session is already inactive is nothing.** There is nothing left to give
//!   back.

use zgui_seat::Change;

/// What a turn of the loop has to do about the session.
///
/// One of these, or nothing at all. There is no third transition: a session holds its devices or
/// does not, and the two edges are the whole vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Transition {
    /// Another session is taking the screen.
    ///
    /// Everything is already gone by the time this is read — the terminal has moved, DRM master
    /// has been dropped and every input descriptor answers `ENODEV` — so this is a record to catch
    /// up with.
    Suspend,
    /// The devices are this session's again.
    ///
    /// The input devices are opened again, every display is put back into its mode, and the
    /// surfaces are told they are visible. Nothing is carried across the switch: `EVIOCREVOKE`
    /// cannot be undone, so an evdev descriptor from before the suspend stays dead, and another
    /// session has set its own mode on every CRTC.
    ///
    /// # What the seat holds on each of the two turns
    ///
    /// A turn that reads an enable after a suspend holds no input device: the suspend gave every
    /// one of them back. A turn that reads a disable and an enable together still holds all of
    /// them, each revoked, because there was no turn in between to give them back in.
    ///
    /// One variant covers both, because the loop's answer is the same: give back whatever is held
    /// and then open every device again. A give-back over an empty list asks the daemon for
    /// nothing, and a device still held here is a revoked descriptor, so the give-back is right on
    /// both turns. Two variants would put that choice back in a caller, which is where it was
    /// wrong.
    Resume,
}

/// Whether this session holds the screen.
///
/// One of these belongs to a running loop. It is given the queue once per turn and answers what to
/// do, and [`Presence::is_active`] is what the rest of the turn reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Presence {
    /// Whether the devices are this session's right now.
    active: bool,
}

impl Presence {
    /// Returns a session that holds the screen.
    ///
    /// A run starts this way on both shapes. A direct run is never told otherwise, because nothing
    /// hands it a change. A seated run started on a terminal that is not the live one is told on
    /// its first turn: the seat leaves the [`Change::Disabled`] that said so in its queue, so
    /// start-up on a background terminal arrives here as an ordinary suspend and the enable that
    /// follows it is an ordinary resume.
    pub(crate) const fn holding() -> Self {
        Self { active: true }
    }

    /// Returns `true` if the devices are this session's.
    ///
    /// While this is false the loop turns and commits nothing: every input device has been given
    /// back and DRM master is another session's. A redraw asked for then is held where it is, and
    /// the resume draws it.
    pub(crate) const fn is_active(self) -> bool {
        self.active
    }

    /// Reads a turn's worth of changes, and answers what to do about them.
    ///
    /// The last change is what says where the session ends up, and the ones in front of it say
    /// whether the devices moved on the way. Both are needed: a queue that ends where it started
    /// still took every device away and gave it back, and a resume is the only thing that opens
    /// them again.
    pub(crate) fn turn(&mut self, changes: &[Change]) -> Option<Transition> {
        let held = self.active;
        self.active = match changes.last()? {
            Change::Enabled => true,
            Change::Disabled => false,
        };

        match (held, self.active) {
            (true, false) => Some(Transition::Suspend),
            (false, true) => Some(Transition::Resume),
            // The devices went and came back inside one turn. Nothing was given back and nothing
            // was told, so what puts this session back is the resume, once — and it runs with
            // every device still held and every one of them revoked.
            (true, true) => changes
                .contains(&Change::Disabled)
                .then_some(Transition::Resume),
            // The session was away and is away. It holds nothing to give back either way.
            (false, false) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Every rule, over queues written here.
    //!
    //! No seat, no device and no descriptor. What a queue holds is what a person did with the
    //! terminals, and a machine cannot be asked to produce a given one. So the orderings that
    //! matter are written out, which is also the one way to reach the ones a switch produces
    //! rarely.

    use super::{Presence, Transition};
    use zgui_seat::Change;

    /// Returns a session that is away, which is a suspend already read.
    fn away() -> Presence {
        let mut presence = Presence::holding();
        assert_eq!(
            presence.turn(&[Change::Disabled]),
            Some(Transition::Suspend)
        );
        presence
    }

    #[test]
    fn a_run_starts_holding_the_screen() {
        assert!(Presence::holding().is_active());
    }

    #[test]
    fn a_turn_that_read_nothing_changes_nothing() {
        // The ordinary turn. A direct run never reads anything else, and a seated one reads nothing
        // between two switches.
        let mut presence = Presence::holding();

        assert_eq!(presence.turn(&[]), None);
        assert!(presence.is_active());

        let mut presence = away();
        assert_eq!(presence.turn(&[]), None);
        assert!(!presence.is_active());
    }

    #[test]
    fn a_disable_takes_the_session_away() {
        let mut presence = Presence::holding();

        assert_eq!(
            presence.turn(&[Change::Disabled]),
            Some(Transition::Suspend)
        );
        assert!(!presence.is_active());
    }

    #[test]
    fn an_enable_brings_it_back() {
        let mut presence = away();

        assert_eq!(presence.turn(&[Change::Enabled]), Some(Transition::Resume));
        assert!(presence.is_active());
    }

    #[test]
    fn two_enables_in_a_row_are_one_resume() {
        // One resume opens every input device again. A second would ask the daemon for a path this
        // run already holds, which seatd answers with the id it answered before and its reference
        // count raised — one record, two names for it, and one give-back.
        let mut presence = away();

        assert_eq!(
            presence.turn(&[Change::Enabled, Change::Enabled]),
            Some(Transition::Resume)
        );
        assert!(presence.is_active());
        assert_eq!(
            presence.turn(&[Change::Enabled]),
            None,
            "and the enable in the turn after it is nothing either"
        );
    }

    #[test]
    fn a_disable_and_an_enable_inside_one_turn_are_a_resume() {
        // A person who switched away and back while the loop was inside one wait. There is no
        // window between the two — the devices were gone before the disable could be read — so
        // what the loop has to do is the resume, once. The seat still holds every device on this
        // path, which is why a resume gives back before it opens: see `crate::app::resume`.
        let mut presence = Presence::holding();

        assert_eq!(
            presence.turn(&[Change::Disabled, Change::Enabled]),
            Some(Transition::Resume)
        );
        assert!(presence.is_active());
    }

    #[test]
    fn a_pair_read_a_turn_apart_is_a_suspend_and_a_resume() {
        // The same two changes, read in two turns, are the two things a switch away and back is.
        // This is what says the rule above is about one turn rather than about the pair.
        let mut presence = Presence::holding();

        assert_eq!(
            presence.turn(&[Change::Disabled]),
            Some(Transition::Suspend)
        );
        assert_eq!(presence.turn(&[Change::Enabled]), Some(Transition::Resume));
        assert!(presence.is_active());
    }

    #[test]
    fn an_enable_while_the_session_is_already_active_is_nothing_at_all() {
        let mut presence = Presence::holding();

        assert_eq!(presence.turn(&[Change::Enabled]), None);
        assert!(presence.is_active());
    }

    #[test]
    fn a_disable_while_the_session_is_already_away_is_nothing() {
        let mut presence = away();

        assert_eq!(presence.turn(&[Change::Disabled]), None);
        assert!(!presence.is_active());
        assert_eq!(
            presence.turn(&[Change::Disabled, Change::Disabled]),
            None,
            "and so is a turn holding two of them"
        );
    }

    #[test]
    fn an_enable_and_a_disable_inside_one_turn_leave_a_holding_session_holding() {
        // The other way round, and the one case where a queue that moved twice asks for nothing: a
        // session that already held the screen was told it holds it, and then that it does not, and
        // ends where a suspend would have put it.
        let mut presence = Presence::holding();

        assert_eq!(
            presence.turn(&[Change::Enabled, Change::Disabled]),
            Some(Transition::Suspend)
        );
        assert!(!presence.is_active());
    }

    #[test]
    fn a_session_that_was_away_and_saw_both_inside_one_turn_stays_away() {
        // It never took its devices back, so there is nothing to give back and nothing was drawn.
        let mut presence = away();

        assert_eq!(presence.turn(&[Change::Enabled, Change::Disabled]), None);
        assert!(!presence.is_active());
    }

    #[test]
    fn a_burst_of_switches_answers_where_it_ended_and_whether_it_moved() {
        // Two switches away and back inside one wait, which holding `Ctrl+Alt+Fn` down produces.
        // The session ends where it started and every device went twice, so one resume puts it
        // back.
        let mut presence = Presence::holding();

        assert_eq!(
            presence.turn(&[
                Change::Disabled,
                Change::Enabled,
                Change::Disabled,
                Change::Enabled,
            ]),
            Some(Transition::Resume)
        );
        assert!(presence.is_active());

        // And the same burst ending the other way is the one suspend.
        assert_eq!(
            presence.turn(&[
                Change::Disabled,
                Change::Enabled,
                Change::Disabled,
                Change::Enabled,
                Change::Disabled,
            ]),
            Some(Transition::Suspend)
        );
        assert!(!presence.is_active());
    }

    #[test]
    fn a_run_that_started_on_a_background_terminal_suspends_and_then_resumes() {
        // What a program started from a terminal nobody is looking at reads. The seat leaves the
        // disable that said so in its queue, so the first turn is an ordinary suspend: it tells the
        // surfaces they are occluded and gives back every input device the daemon handed over
        // revoked. The enable that arrives when a person switches to that terminal is an ordinary
        // resume.
        let mut presence = Presence::holding();

        assert_eq!(
            presence.turn(&[Change::Disabled]),
            Some(Transition::Suspend)
        );
        assert!(!presence.is_active());
        assert_eq!(presence.turn(&[]), None, "and it waits");
        assert_eq!(presence.turn(&[Change::Enabled]), Some(Transition::Resume));
        assert!(presence.is_active());
    }
}
