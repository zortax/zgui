//! One open seat: the descriptor to wait on, and what happened to it.

use std::ffi::{CStr, c_int};
use std::fmt;
use std::marker::PhantomData;
use std::os::fd::{BorrowedFd, RawFd};
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::library::{Library, Libseat};
use crate::seat::listener::{LISTENER, Shared};

mod listener;

pub use crate::seat::listener::Change;

/// How long [`Seat::open`] waits for the seat to enable.
///
/// The bound tells a seat this session got from one it did not get. `libseat_open_seat` answers a
/// handle as soon as a backend accepts the call, and the builtin backend accepts it with no
/// terminal it can take, so a seat that never enables arrives as a success. libseat names the
/// backend that answered in its own log and offers no way to ask, so a caller reads the wait
/// running out instead.
pub const ENABLE_WITHIN: Duration = Duration::from_secs(2);

/// How long one dispatch inside that wait stops for, in milliseconds.
///
/// A step rather than the whole bound, so that the elapsed time is read often enough for the bound
/// to mean what it says.
const STEP: c_int = 25;

/// `libseat_dispatch` with no wait at all.
const NO_WAIT: c_int = 0;

/// A seat, open and enabled.
///
/// The seat owns the devices this session may use. [`Seat::descriptor`] is what a loop waits on,
/// and [`Seat::dispatch`] turns what arrived into [`Change`]s.
///
/// # Closing
///
/// A session daemon holds the terminal for as long as the client that took control of it lives, and
/// gives it back when that client goes. The seat is therefore closed by `Drop` rather than by a
/// call at the end of a function, so a program that panics while it holds a terminal gives the
/// terminal back as it unwinds.
///
/// # One thread
///
/// A `Seat` stays on the thread that opened it. libseat dispatches on the thread that calls it and
/// its state is shared with nothing, so the marker on this type says the same.
pub struct Seat {
    /// The seat, and everything that is given back when it closes.
    held: Held,
    /// The name, copied at the open.
    name: String,
    /// The connection descriptor, read at the open.
    descriptor: RawFd,
    /// What makes this type `!Send` and `!Sync`.
    ///
    /// The raw pointers inside [`Held`] do this as well. The marker states it, so that a field
    /// which becomes something shareable cannot make the whole type shareable with it.
    thread_bound: PhantomData<*const ()>,
}

impl Seat {
    /// Opens the seat this session is on, and waits for it to enable.
    ///
    /// The wait is bounded by [`ENABLE_WITHIN`]. `LIBSEAT_BACKEND` names the backend to use, and
    /// without it libseat tries each backend it was built with and takes the first that opens a
    /// seat.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Library`] or [`Error::Symbol`] when libseat cannot be opened,
    /// [`Error::Seat`] when libseat refused the seat, [`Error::Dispatch`] when libseat could not
    /// read its connection while the wait ran, and [`Error::NeverEnabled`] when the seat opened and
    /// did not enable inside the bound.
    pub fn open() -> Result<Self> {
        let held = Held::open(Library::load()?)?;
        held.wait_for_enable()?;

        let descriptor = held.descriptor()?;
        let name = held.name();

        Ok(Self {
            held,
            name,
            descriptor,
            thread_bound: PhantomData,
        })
    }

    /// Returns the seat's name, such as `seat0`.
    ///
    /// The name is copied when the seat opens. libseat lends a string it owns, and a borrow of it
    /// could only be handed back as a `&str` for a name that is UTF-8, so the copy makes this
    /// answer hold for every name. A backend that answers no name at all gives an empty one.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the descriptor to wait on.
    ///
    /// It becomes readable when libseat has something for [`Seat::dispatch`]. Nothing else is read
    /// from it, and it is closed when the seat closes.
    pub fn descriptor(&self) -> BorrowedFd<'_> {
        // SAFETY: `libseat_get_fd` answered this descriptor for this seat, and `libseat_close_seat`
        // is what closes it, in the `Drop` below. The borrow is tied to `&self`, so it ends before
        // that.
        unsafe { BorrowedFd::borrow_raw(self.descriptor) }
    }

    /// Reads what has arrived, and answers what changed.
    ///
    /// This waits for nothing: it reads what is there and returns. A caller that has nothing else
    /// to do waits on [`Seat::descriptor`] first.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Dispatch`] when libseat could not read its connection.
    pub fn dispatch(&mut self) -> Result<Vec<Change>> {
        self.held.turn(NO_WAIT)?;
        Ok(self.held.shared().take())
    }
}

/// Says which seat this is, without listing addresses.
impl fmt::Debug for Seat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Seat")
            .field("name", &self.name)
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

/// An open seat, and everything that has to be given back when it closes.
///
/// This is apart from [`Seat`] so that every step between the open and a usable seat can fail and
/// still give the seat back: the `Drop` below runs from the moment libseat accepted the open.
struct Held {
    /// The library the addresses below are called through.
    library: Library,
    /// libseat's own handle.
    handle: NonNull<Libseat>,
    /// The state the callbacks reach through, leaked at the open and reclaimed at the close.
    shared: NonNull<Shared>,
}

impl Held {
    /// Opens a seat over an open library.
    ///
    /// The state exists before the call, because the seatd and builtin backends report the first
    /// enable from inside it.
    fn open(library: Library) -> Result<Self> {
        let shared = NonNull::from(Box::leak(Box::new(Shared::new(library.symbols()))));

        // SAFETY: `LISTENER` is a `static`, so the address libseat keeps stays valid for longer than
        // any seat. `shared` is the box leaked above, which is reclaimed in `Drop` after the seat is
        // closed, so it stands for as long as libseat can call through it.
        let handle =
            unsafe { (library.symbols().open_seat)(&raw const LISTENER, shared.as_ptr().cast()) };

        let Some(handle) = NonNull::new(handle) else {
            let errno = errno();

            // SAFETY: the box leaked above, reclaimed here. The open failed, so libseat kept
            // nothing that points at it.
            drop(unsafe { Box::from_raw(shared.as_ptr()) });

            return Err(Error::Seat {
                call: "libseat_open_seat",
                errno,
            });
        };

        Ok(Self {
            library,
            handle,
            shared,
        })
    }

    /// Waits for the seat to enable, up to [`ENABLE_WITHIN`].
    ///
    /// Two halves, and each covers a different backend. The queue is read first, because the seatd
    /// and builtin backends make the call from inside `libseat_open_seat`. The wait then dispatches,
    /// because the logind and noop backends set a flag during the open and make the call from the
    /// first dispatch. A caller that did one of the two works on half the machines.
    fn wait_for_enable(&self) -> Result<()> {
        let started = Instant::now();
        loop {
            if self.shared().take_through_enable() {
                return Ok(());
            }
            if started.elapsed() >= ENABLE_WITHIN {
                return Err(Error::NeverEnabled {
                    within: ENABLE_WITHIN,
                });
            }
            self.turn(STEP)?;
        }
    }

    /// The descriptor libseat waits on.
    fn descriptor(&self) -> Result<RawFd> {
        // SAFETY: `handle` is the seat libseat gave back, and it is open until `Drop`.
        let answer = unsafe { (self.library.symbols().get_fd)(self.handle.as_ptr()) };

        if answer < 0 {
            return Err(Error::Seat {
                call: "libseat_get_fd",
                errno: errno(),
            });
        }
        Ok(answer)
    }

    /// The seat's name, copied out of libseat.
    fn name(&self) -> String {
        // SAFETY: `handle` is the seat libseat gave back, and it is open until `Drop`.
        let name = unsafe { (self.library.symbols().seat_name)(self.handle.as_ptr()) };

        if name.is_null() {
            return String::new();
        }

        // SAFETY: libseat answers a NUL-terminated string that belongs to the seat and is valid for
        // as long as it is open, which is longer than this call. The copy is made here, so nothing
        // later reads through the pointer.
        unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned()
    }

    /// Reads what has arrived, waiting `timeout` milliseconds for something to.
    ///
    /// The count libseat answers is dropped on purpose. The logind and noop backends report the
    /// first enable without counting it, so a caller that read the count would decide a seat had
    /// said nothing while holding the call that said it. What the callbacks recorded is the answer.
    fn turn(&self, timeout: c_int) -> Result<()> {
        // SAFETY: `handle` is the seat libseat gave back, and it is open until `Drop`. libseat runs
        // the listener from inside this call; the callbacks take their own borrow of the queue and
        // this function holds none.
        let answer = unsafe { (self.library.symbols().dispatch)(self.handle.as_ptr(), timeout) };

        if answer < 0 {
            return Err(Error::Dispatch { errno: errno() });
        }
        Ok(())
    }

    /// The state the callbacks push onto.
    fn shared(&self) -> &Shared {
        // SAFETY: the box leaked in `open` and is reclaimed in `Drop`, so it stands for the life of
        // this value. Nothing takes a `&mut` to it: this and the callbacks take shared references,
        // and what changes sits behind a `RefCell`.
        unsafe { self.shared.as_ref() }
    }
}

/// Gives the seat back, and then the state its callbacks reached through.
///
/// The order matters. `libseat_close_seat` frees libseat's own structure, and no callback can run
/// afterwards, so the box is reclaimed second. `library` is dropped after this body, because every
/// address called here points inside its mapping.
impl Drop for Held {
    fn drop(&mut self) {
        // SAFETY: `handle` is the seat libseat gave back, closed once, here. Nothing calls through
        // it afterwards.
        unsafe { (self.library.symbols().close_seat)(self.handle.as_ptr()) };

        // SAFETY: the box leaked in `open`, reclaimed once, here. The seat is closed, so libseat
        // holds nothing that points at it.
        drop(unsafe { Box::from_raw(self.shared.as_ptr()) });
    }
}

/// What the last call set `errno` to.
///
/// libseat puts the failure in the return value and the reason here, so this is read straight after
/// the call that failed.
fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}
