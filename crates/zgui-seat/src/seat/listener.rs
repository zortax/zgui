//! What libseat calls back into.
//!
//! libseat reports a seat's changes through two C function pointers. It makes those calls from
//! inside `libseat_open_seat` and from inside `libseat_dispatch`, on the thread that called, and it
//! gives each one the `userdata` the seat was opened with. So the two calls below take the state
//! they push onto out of that pointer.
//!
//! # The listener address
//!
//! Every libseat backend stores the listener pointer and reads through it on each later event.
//! Nothing is copied. [`LISTENER`] is therefore a `static`: an address that stands for as long as
//! the process does, which is longer than any seat opened over it. A listener built on the stack of
//! the function that opens a seat leaves a dangling pointer at the first terminal switch.
//!
//! # Borrows across a libseat call
//!
//! The queue sits behind a `RefCell`, and libseat runs these calls from inside its own. Each method
//! below takes its borrow and gives it back inside itself, and the disable call pushes before it
//! acknowledges, so a callback that runs inside another libseat call always finds the cell free.

use std::cell::RefCell;
use std::ffi::{c_int, c_void};

use crate::library::{Libseat, SeatListener, Symbols};

/// libseat's `libseat_disable_seat`, as [`disabled`] reaches it.
///
/// A callback cannot reach the caller's [`crate::Library`], so the address travels in [`Shared`].
type Acknowledge = unsafe extern "C" fn(*mut Libseat) -> c_int;

/// What happened to a seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The session holds its devices again, and every one of them is opened again.
    ///
    /// A descriptor from before the last [`Change::Disabled`] can have been blocked or revoked, so
    /// a caller reopens rather than reusing.
    Enabled,
    /// Another session is taking the devices.
    ///
    /// They stop answering, and they stay unusable until the next [`Change::Enabled`]. The seat has
    /// already acknowledged this by the time a caller reads it.
    Disabled,
}

/// The state the two calls below reach through `userdata`.
///
/// One seat owns one of these. It is boxed, the pointer is what libseat carries, and the seat
/// reclaims it when it closes.
pub(crate) struct Shared {
    /// What arrived, in the order it arrived, waiting to be read.
    changes: RefCell<Vec<Change>>,
    /// How [`disabled`] acknowledges.
    acknowledge: Acknowledge,
}

impl Shared {
    /// The state a seat starts with, over the addresses its callbacks need.
    pub(crate) fn new(symbols: &Symbols) -> Self {
        Self {
            changes: RefCell::new(Vec::new()),
            acknowledge: symbols.disable_seat,
        }
    }

    /// Takes everything that has arrived.
    pub(crate) fn take(&self) -> Vec<Change> {
        std::mem::take(&mut *self.changes.borrow_mut())
    }

    /// What the seat has said so far, with everything up to a first [`Change::Enabled`] consumed.
    ///
    /// Three answers, and a seat that is opening is waiting for one of them.
    ///
    /// [`Change::Enabled`] is a seat that holds its devices. What it did before that is the seat
    /// becoming usable, so it is consumed; what follows the enable is the caller's and stays in the
    /// queue.
    ///
    /// [`Change::Disabled`] with no enable behind it is a seat whose session is not the live one.
    /// The change **stays in the queue**, because it is the caller's: it says the seat is open and
    /// waiting, and the caller reads it the way it reads every later one.
    ///
    /// Nothing at all is a seat that has said nothing yet.
    pub(crate) fn first_answer(&self) -> Option<Change> {
        let mut changes = self.changes.borrow_mut();
        match changes.iter().position(|change| *change == Change::Enabled) {
            Some(first) => {
                changes.drain(..=first);
                Some(Change::Enabled)
            }
            None => changes.first().copied(),
        }
    }

    /// Records one change.
    fn push(&self, change: Change) {
        self.changes.borrow_mut().push(change);
    }
}

/// The listener every seat this crate opens is given.
///
/// libseat keeps this address for the life of the seat. The two fields are function pointers, which
/// are `Sync` on their own, so the `static` carries no promise of its own.
pub(crate) static LISTENER: SeatListener = SeatListener {
    enable_seat: enabled,
    disable_seat: disabled,
};

/// libseat's `enable_seat`.
///
/// # Safety
///
/// libseat calls this. `userdata` is the pointer the seat was opened with, which is a [`Shared`]
/// the seat keeps until after it closes.
unsafe extern "C" fn enabled(_seat: *mut Libseat, userdata: *mut c_void) {
    // SAFETY: the seat gave `libseat_open_seat` a pointer to its own boxed `Shared`, and libseat
    // hands that pointer back unchanged. The box outlives every call, because the seat frees it
    // after `libseat_close_seat`, and no callback runs afterwards. Nothing holds a `&mut` to it.
    let shared = unsafe { &*userdata.cast::<Shared>() };

    shared.push(Change::Enabled);
}

/// libseat's `disable_seat`, which also acknowledges.
///
/// The acknowledgement is made here so that it is never late: a seat that is slow to answer has its
/// devices taken from it. The seat pointer is libseat's own argument, because a callback has no way
/// to reach the one the caller holds.
///
/// # Safety
///
/// libseat calls this. `seat` is the seat the change is about, and `userdata` is the pointer the
/// seat was opened with, which is a [`Shared`] the seat keeps until after it closes.
unsafe extern "C" fn disabled(seat: *mut Libseat, userdata: *mut c_void) {
    // SAFETY: as in `enabled`, and for the same reasons.
    let shared = unsafe { &*userdata.cast::<Shared>() };

    // The borrow inside `push` is given back here, before the call below. libseat may report
    // another change from inside that call, and the callback that runs then takes its own.
    shared.push(Change::Disabled);

    // SAFETY: `seat` is the seat libseat is reporting about, so it is open for the length of this
    // call, and `acknowledge` is `libseat_disable_seat` out of the library the seat holds open. The
    // answer says whether libseat recorded the acknowledgement; a seat that is being taken away has
    // nothing to do about a refusal, and the loss arrives as the devices failing.
    unsafe { (shared.acknowledge)(seat) };
}

#[cfg(test)]
mod tests {
    //! What an opening seat reads out of the queue, over queues written here.
    //!
    //! No libseat and no seat. The queue is a `Vec` the two callbacks push onto, so the three
    //! answers an open can get are reachable by writing the `Vec`. Two of them are reachable that
    //! way alone: libseat's noop backend enables at once and has no session to be inactive on, so a
    //! seat that opens inactive and a seat that says nothing are the machine's answer rather than
    //! anything a test can ask for.

    use super::{Change, Shared};
    use crate::library::Libseat;
    use std::cell::RefCell;
    use std::ffi::c_int;

    /// An acknowledgement that goes nowhere.
    ///
    /// [`Shared`] holds `libseat_disable_seat` so that the disable callback can reach it. Nothing
    /// below runs a callback, so this stands in the field and is called by nothing.
    unsafe extern "C" fn unreachable(_seat: *mut Libseat) -> c_int {
        0
    }

    /// A queue holding `changes`, as the callbacks would have left it.
    fn queued(changes: &[Change]) -> Shared {
        Shared {
            changes: RefCell::new(changes.to_vec()),
            acknowledge: unreachable,
        }
    }

    #[test]
    fn a_seat_that_has_said_nothing_answers_nothing_and_is_waited_for() {
        let shared = queued(&[]);

        assert_eq!(shared.first_answer(), None);
        assert!(shared.take().is_empty(), "and nothing was invented");
    }

    #[test]
    fn an_enable_is_consumed_and_what_follows_it_is_the_callers() {
        // The seat becoming usable is the open's own business. A change that arrived after it
        // belongs to the caller, and a queue emptied here would lose a switch that happened while
        // the seat was still opening.
        let shared = queued(&[Change::Enabled, Change::Disabled]);

        assert_eq!(shared.first_answer(), Some(Change::Enabled));
        assert_eq!(
            shared.take(),
            [Change::Disabled],
            "the change behind the enable is still there to read"
        );
    }

    #[test]
    fn a_disable_in_front_of_an_enable_is_consumed_with_it() {
        // A session that was inactive when the seat opened and became active during the open. Both
        // are the seat becoming usable, and neither is a transition the caller has to act on.
        let shared = queued(&[Change::Disabled, Change::Enabled]);

        assert_eq!(shared.first_answer(), Some(Change::Enabled));
        assert!(shared.take().is_empty());
    }

    #[test]
    fn a_disable_with_no_enable_behind_it_stays_in_the_queue() {
        // logind reads whether the session is active while the seat is opening and reports an
        // inactive one as disabled on the first dispatch. That change says the seat is open and
        // waiting for its terminal, so the caller reads it the way it reads every later one. A
        // queue cleared here would leave a session that believes it holds a screen another session
        // is on.
        let shared = queued(&[Change::Disabled]);

        assert_eq!(shared.first_answer(), Some(Change::Disabled));
        assert_eq!(
            shared.take(),
            [Change::Disabled],
            "the caller's first dispatch is what acts on it"
        );
    }
}
