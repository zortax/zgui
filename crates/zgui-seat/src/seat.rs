//! One open seat: the descriptor to wait on, what happened to it, and the devices it opens.

use std::ffi::{CStr, CString, c_int};
use std::fmt;
use std::marker::PhantomData;
use std::os::fd::{BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::library::{Library, Libseat};
use crate::seat::device::Token;
use crate::seat::listener::{LISTENER, Shared};

mod device;
mod listener;

pub use crate::seat::device::Device;
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

    /// Opens one device on this seat.
    ///
    /// The seat opens the device and hands the descriptor over. Which devices it hands over is the
    /// backend's own rule: seatd and logind take graphics cards and input devices and refuse the
    /// rest, and the noop backend opens any path it is given. seatd also refuses every device while
    /// the session is inactive.
    ///
    /// Every device is opened again after a [`Change::Enabled`]. A descriptor from before the
    /// disable can have been blocked or revoked, and an evdev one always has been.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OpenDevice`], which names the path, when libseat refused, and
    /// [`Error::DevicePath`] for a path that holds a zero byte.
    pub fn open_device(&self, path: &Path) -> Result<Device> {
        self.held.open_device(path)
    }

    /// Gives one device back, and closes its descriptor.
    ///
    /// The device belongs to the seat that opened it. A seat given another seat's device refuses
    /// it, because a device id belongs to the seat that answered it.
    ///
    /// # Order
    ///
    /// libseat is told first and the descriptor is closed second. logind's backend keeps the
    /// descriptor's own number as the device id and stats it to find which device to release, so a
    /// descriptor that went first would release the wrong device, or none.
    ///
    /// # Errors
    ///
    /// Returns [`Error::CloseDevice`] when libseat refused, and [`Error::OtherSeat`] for a device
    /// another seat opened. The descriptor is closed either way, because libseat closes none of
    /// them and this is the last owner.
    pub fn close_device(&self, device: Device) -> Result<()> {
        // The call below borrows the device, so the descriptor is open for the whole of it. The
        // borrow holds that order: a body that closed the descriptor first has moved the device,
        // and the call that follows it does not compile. An id copied out ahead of the call would
        // make the other order compile, because an id is a `c_int`, so the id stays reachable
        // inside this crate alone.
        let answer = self.held.close_device(&device);

        // The descriptor closes here, which is after the call above.
        drop(device);

        answer
    }

    /// Asks the seat to switch to another terminal.
    ///
    /// A seat bound to terminals numbers its sessions the way the terminals are numbered, so this
    /// is how a terminal is asked for. logind switches to a terminal that holds no session as well,
    /// so a getty is reachable this way.
    ///
    /// A seat asks for this from an inactive session too, and that is the only way back: the
    /// console keyboard stops answering while a session daemon holds the terminal, so a program
    /// that took a seat comes back through this call.
    ///
    /// The answer says that the request went out. A switch can still fail to happen, so a caller
    /// carries on as though the session is unchanged, and learns that it moved from
    /// [`Change::Disabled`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Switch`] when libseat refused, which a backend with no terminals does for
    /// every switch, and [`Error::Terminal`] for a number wider than libseat's interface holds.
    pub fn switch(&self, terminal: u32) -> Result<()> {
        self.held.switch(terminal)
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
    /// Which seat this is, which every device it opens carries.
    token: Token,
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
            token: Token::next(),
        })
    }

    /// Waits for the seat to enable, up to [`ENABLE_WITHIN`].
    ///
    /// Two halves, and each covers a different backend. The queue is read first, because the seatd
    /// and builtin backends make the call from inside `libseat_open_seat`. The wait then dispatches,
    /// because the logind and noop backends set a flag during the open and make the call from the
    /// first dispatch. A caller that did one of the two works on half the machines.
    ///
    /// The first dispatch waits for nothing. Both of those backends make the call at the top of
    /// their dispatch and then spend what is left of the timeout waiting for a message that has
    /// already arrived, so a first step would cost its whole length on every open. Later dispatches
    /// carry the step, which is what keeps the loop off the processor.
    fn wait_for_enable(&self) -> Result<()> {
        let started = Instant::now();
        let mut timeout = NO_WAIT;
        loop {
            if self.shared().take_through_enable() {
                return Ok(());
            }
            if started.elapsed() >= ENABLE_WITHIN {
                return Err(Error::NeverEnabled {
                    within: ENABLE_WITHIN,
                });
            }
            self.turn(timeout)?;
            timeout = STEP;
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

    /// Opens one device, and takes over the descriptor libseat wrote.
    ///
    /// libseat answers an id and writes the descriptor through the pointer, and the two are
    /// separate numbers on the seatd backend. Both are kept.
    fn open_device(&self, path: &Path) -> Result<Device> {
        let held = CString::new(path.as_os_str().as_bytes()).map_err(|_| Error::DevicePath {
            path: path.to_owned(),
        })?;
        let mut descriptor: c_int = -1;

        // SAFETY: `handle` is the seat libseat gave back, and it is open until `Drop`. `held` is a
        // NUL-terminated string that stands for the length of the call, and libseat copies what it
        // needs of the path. `descriptor` is one `int` owned by this frame, and libseat writes
        // the descriptor through it.
        let id = unsafe {
            (self.library.symbols().open_device)(
                self.handle.as_ptr(),
                held.as_ptr(),
                &raw mut descriptor,
            )
        };

        // Read once, here, because the answer is checked in two steps, the give-back below makes a
        // call of its own, and this number belongs to the call above.
        let errno = errno();

        // A backend that answered an id and wrote no descriptor lands here as well. `OwnedFd` may
        // not hold `-1`, which is an invalid value for the type and is undefined the moment one
        // exists, so what libseat wrote is read rather than trusted.
        if id < 0 || descriptor < 0 {
            if id >= 0 {
                // libseat took the device and this call answers no `Device`, so this is the last
                // place that holds the id. The device is given straight back.
                //
                // No backend reaches this. seatd, logind and noop each answer `-1` and write
                // nothing through the pointer, so nothing here can make this branch run and no test
                // covers it: deleting it, or the `descriptor < 0` half of the condition above,
                // leaves the suite green.
                //
                // SAFETY: `handle` is the seat libseat gave back, and it is open until `Drop`. The
                // id is the one the call above answered. The answer is dropped: this function
                // already has a failure to report.
                unsafe { (self.library.symbols().close_device)(self.handle.as_ptr(), id) };
            }

            return Err(Error::OpenDevice {
                path: path.to_owned(),
                errno,
            });
        }

        // SAFETY: libseat opened this descriptor for this call and closes no descriptor of its own,
        // so nothing else owns it. It is not `-1`, which the branch above settled.
        let descriptor = unsafe { OwnedFd::from_raw_fd(descriptor) };

        Ok(Device::new(self.token, id, descriptor))
    }

    /// Tells libseat that one device is free.
    ///
    /// The device is borrowed, so its descriptor is open for the length of the call. logind's
    /// backend stats that descriptor to find which device to release.
    ///
    /// A device another seat opened is refused here. Its id is that seat's, and this seat numbers
    /// its own devices, so the call would release one of these or none of them.
    fn close_device(&self, device: &Device) -> Result<()> {
        if device.seat() != self.token {
            return Err(Error::OtherSeat {
                device: device.id(),
            });
        }

        // SAFETY: `handle` is the seat libseat gave back, and it is open until `Drop`. The id is
        // the one libseat answered for this device, and the descriptor behind it is open, because
        // the borrow above holds the device for the length of this call.
        let answer =
            unsafe { (self.library.symbols().close_device)(self.handle.as_ptr(), device.id()) };

        if answer < 0 {
            return Err(Error::CloseDevice {
                device: device.id(),
                errno: errno(),
            });
        }
        Ok(())
    }

    /// Asks for a terminal.
    ///
    /// The number crosses as a C `int`. A terminal number is never negative, so [`Seat::switch`]
    /// takes a `u32`, and one wider than the `int` is refused here. Every such number crosses as a
    /// negative one, which each backend refuses on its own, so what this adds is a refusal that
    /// names the number and asks libseat nothing.
    fn switch(&self, terminal: u32) -> Result<()> {
        let session = c_int::try_from(terminal).map_err(|_| Error::Terminal { terminal })?;

        // SAFETY: `handle` is the seat libseat gave back, and it is open until `Drop`.
        let answer =
            unsafe { (self.library.symbols().switch_session)(self.handle.as_ptr(), session) };

        if answer < 0 {
            return Err(Error::Switch {
                terminal,
                errno: errno(),
            });
        }
        Ok(())
    }

    /// Reads what has arrived, waiting `timeout` milliseconds for something to.
    ///
    /// The count libseat answers is dropped on purpose. The logind and noop backends report the
    /// first enable without counting it, so a caller that read the count would decide a seat had
    /// said nothing while holding the call that said it. What the callbacks recorded is the answer.
    fn turn(&self, timeout: c_int) -> Result<()> {
        // `-1` is libseat's "wait for as long as it takes". A seat holds the terminal, so a program
        // that stops for ever inside this call leaves a terminal nobody can use, and nothing later
        // reports it: no answer comes back and no assertion fires. Every caller here passes a
        // bound, and this assertion states it.
        debug_assert!(
            timeout >= 0,
            "libseat_dispatch waits without end on a negative timeout"
        );

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
