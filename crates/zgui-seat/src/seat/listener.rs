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

    /// Takes everything up to and including the first [`Change::Enabled`], and says whether one was
    /// there.
    ///
    /// What a seat did before it first enabled is the seat becoming usable, so it is consumed. What
    /// follows the enable is the caller's and stays in the queue.
    pub(crate) fn take_through_enable(&self) -> bool {
        let mut changes = self.changes.borrow_mut();
        match changes.iter().position(|change| *change == Change::Enabled) {
            Some(first) => {
                changes.drain(..=first);
                true
            }
            None => {
                changes.clear();
                false
            }
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
