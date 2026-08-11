//! Who opens a device, and who takes it back.
//!
//! libinput opens nothing. Every device it reads is opened by its caller and handed over as a
//! descriptor, and given back the same way. That is the seam a session daemon fits into: the daemon
//! owns the device, this process is lent it, and the lending survives a terminal switch because
//! libinput asks again by path rather than holding what it was given.
//!
//! # Lending
//!
//! [`Files`] is answered with the session the devices come from, and a session is the thing the
//! rest of a program is also holding. So it is not kept here. Each [`Context`](super::Context)
//! call that libinput can call back from takes the caller as an argument, makes it reachable for
//! exactly that call, and takes it away again.
//!
//! libinput calls back from inside the call that was made, on the thread that made it, and from
//! nowhere else. A pointer that is set for the length of one call is therefore valid whenever it is
//! read.

use std::cell::Cell;
use std::ffi::{CStr, OsStr, c_char, c_int, c_void};
use std::os::fd::{FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr::NonNull;

use crate::library::Interface;

/// Linux's `EINVAL`, written out.
///
/// This crate has no C library binding, and the two numbers below are the two it needs. Both have
/// held since the first Linux release, and libinput puts either of them in its own log and acts on
/// neither.
const EINVAL: c_int = 22;

/// Linux's `EIO`, written out.
const EIO: c_int = 5;

/// Where libinput's devices are opened.
///
/// The two calls are libinput's `open_restricted` and `close_restricted`. A caller that opens the
/// nodes itself answers them with `open` and a close; a caller on a session daemon answers them by
/// asking the daemon, so an ordinary user can reach a keyboard.
pub trait Files {
    /// Opens the device at `path`, or refuses with a positive `errno`.
    ///
    /// `flags` is what libinput would have opened with. Answering with a **narrower access mode**
    /// is allowed: a read-only descriptor is accepted, and gives up the writes libinput makes to a
    /// device, which are its lights.
    ///
    /// # Non-blocking descriptors
    ///
    /// libinput reads the descriptor directly, on the thread that calls
    /// [`Context::dispatch`](super::Context), and it reads until the device has nothing more to
    /// say. A descriptor without `O_NONBLOCK` therefore holds that thread inside libinput for the
    /// rest of the run: no event is reported, and the program is still running. `flags` already
    /// carries the bit, so an implementation that passes `flags` through to the open has nothing
    /// else to do.
    ///
    /// libinput puts a refusal in its own log and takes no other decision from it, so a caller with
    /// no number to give can answer any of them.
    ///
    /// # Errors
    ///
    /// The `errno` the open failed with. `0` is reported to libinput as `EIO`, because a refusal
    /// that reads as success would be taken for the descriptor numbered zero.
    fn open(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32>;

    /// Takes back a descriptor [`Files::open`] answered with.
    ///
    /// The descriptor is handed over rather than closed, so that a caller which has to tell
    /// somebody else — a session daemon holding its own record of the device — can do that before
    /// it goes. Dropping it closes it.
    fn close(&mut self, fd: OwnedFd);
}

/// How to reach [`Files::open`] on a caller whose type has been forgotten.
type Opens = unsafe fn(*mut c_void, &Path, c_int) -> Result<OwnedFd, i32>;

/// How to reach [`Files::close`] on a caller whose type has been forgotten.
type Closes = unsafe fn(*mut c_void, OwnedFd);

/// One caller, reachable for the length of one call.
#[derive(Clone, Copy)]
struct Lent {
    /// The caller, with its type forgotten.
    files: NonNull<c_void>,
    /// What `files` points at, enough of it to make the two calls.
    opens: Opens,
    /// The other half of the same answer.
    closes: Closes,
}

/// What the two callbacks find behind libinput's `user_data`.
///
/// One context owns one of these. It is boxed, the pointer is what libinput carries, and the
/// context reclaims it when it is freed.
///
/// Everything here is reached through a shared reference. libinput calls back from inside a call
/// the context is making, so a `&mut` taken by the context and a `&mut` taken by the callback would
/// be two at once. The cell makes each of them one shared reference, and it holds a value that is
/// [`Copy`], so reading it leaves no borrow open across the call that follows.
pub(crate) struct Callers {
    /// The caller of the call being made, and nothing between calls.
    lent: Cell<Option<Lent>>,
}

impl Callers {
    /// Creates one with nobody lent.
    pub(crate) fn new() -> Self {
        Self {
            lent: Cell::new(None),
        }
    }

    /// Makes `files` reachable from the two callbacks.
    ///
    /// The caller of this pairs it with [`Callers::take_back`] around one call into libinput, and
    /// around nothing else.
    pub(crate) fn lend<F: Files>(&self, files: &mut F) {
        /// Reaches [`Files::open`] on an `F` again.
        ///
        /// This is generic, so the type is recovered rather than assumed: what erased the type and
        /// what restores it are the same instantiation.
        unsafe fn opens<F: Files>(
            files: *mut c_void,
            path: &Path,
            flags: c_int,
        ) -> Result<OwnedFd, i32> {
            // SAFETY: `files` is the pointer `lend` erased from a `&mut F`, and this is the `F` it
            // was erased from — the two are one instantiation of this function. The borrow it came
            // from outlives the call, because `lend` is paired with `take_back` inside the call
            // that borrowed it.
            let files = unsafe { &mut *files.cast::<F>() };
            files.open(path, flags)
        }

        /// Reaches [`Files::close`] on an `F` again.
        unsafe fn closes<F: Files>(files: *mut c_void, fd: OwnedFd) {
            // SAFETY: as `opens` above.
            let files = unsafe { &mut *files.cast::<F>() };
            files.close(fd);
        }

        self.lent.set(Some(Lent {
            files: NonNull::from(files).cast::<c_void>(),
            opens: opens::<F>,
            closes: closes::<F>,
        }));
    }

    /// Takes the caller away again.
    ///
    /// After this the callbacks reach nobody. A descriptor handed back outside any call is closed
    /// here rather than given to somebody who is no longer in a call.
    pub(crate) fn take_back(&self) {
        self.lent.set(None);
    }
}

/// The two calls libinput opens and closes devices through.
///
/// A `static` because libinput keeps the pointer rather than the value: every device opened after
/// the context is made, including every device opened again by a resume, is opened through this
/// address. See [`Interface`].
pub(crate) static INTERFACE: Interface = Interface {
    open_restricted: opened,
    close_restricted: closed,
};

/// libinput's `open_restricted`, answered by whoever is lent.
unsafe extern "C" fn opened(path: *const c_char, flags: c_int, user_data: *mut c_void) -> c_int {
    // An unwind out of this frame would run into C, which is undefined. A caller that panics is
    // reported to libinput as `EIO`, and the panic ends here.
    let answered = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: `user_data` is the address of the `Callers` the context leaked, which the context
        // reclaims only after libinput has been freed. The reference is shared, and every other
        // reference to it is shared as well, so no borrow rule is broken by libinput calling this
        // from inside a call the context is making.
        let Some(callers) = (unsafe { user_data.cast::<Callers>().as_ref() }) else {
            return -EINVAL;
        };
        let Some(lent) = callers.lent.get() else {
            // Nobody is lent, so nobody can open anything. libinput opens devices from inside the
            // calls that lend, so this is unreachable rather than a case with an answer.
            return -EINVAL;
        };
        if path.is_null() {
            return -EINVAL;
        }
        // SAFETY: libinput passes the path it was given, as a C string it owns, and it stays valid
        // for the length of this call. The bytes are copied into a `Path` borrow that lives no
        // longer than this frame.
        let bytes = unsafe { CStr::from_ptr(path) }.to_bytes();
        let path = Path::new(OsStr::from_bytes(bytes));

        // SAFETY: `lent` was built by `lend` out of a `&mut F` and the two shims for that same `F`.
        match unsafe { (lent.opens)(lent.files.as_ptr(), path, flags) } {
            // libinput takes the descriptor and hands it back through `closed`, so ownership
            // leaves Rust here rather than being dropped at the end of this frame.
            Ok(fd) => fd.into_raw_fd(),
            // A refusal is negative. Zero would be read as the descriptor numbered zero, so it is
            // answered as `EIO`. The most negative number saturates to `i32::MAX`, so it is
            // answered as `-i32::MAX`.
            Err(errno) => match errno.saturating_abs() {
                0 => -EIO,
                positive => -positive,
            },
        }
    }));
    answered.unwrap_or(-EIO)
}

/// libinput's `close_restricted`, answered by whoever is lent.
unsafe extern "C" fn closed(fd: c_int, user_data: *mut c_void) {
    // The unwind is caught for the reason `opened` gives. A caller that panics has already had
    // the descriptor taken from it, so the descriptor is closed either way.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if fd < 0 {
            return;
        }
        // SAFETY: libinput hands back a descriptor `opened` answered with and forgets it, so this
        // is the one owner of it. Building the owner first is what closes it however this frame
        // ends.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };

        // SAFETY: as `opened` above.
        let Some(callers) = (unsafe { user_data.cast::<Callers>().as_ref() }) else {
            return;
        };
        match callers.lent.get() {
            // SAFETY: as `opened` above.
            Some(lent) => unsafe { (lent.closes)(lent.files.as_ptr(), fd) },
            // Nobody is lent. This is the drop of a context that was never closed through its
            // caller: the descriptor is this process's own, so closing it is all that is owed, and
            // whoever else holds a record of the device is told by their own drop.
            None => drop(fd),
        }
    }));
}
