//! Opening libinput at run time, and the symbols taken out of it.
//!
//! Nothing in this crate is linked against libinput. The shared object is opened with `dlopen`,
//! every symbol is resolved once, and the addresses are kept beside the open handle. A build
//! therefore needs neither the library nor a device, and a machine without either answers
//! [`Library::load`] with [`Error::Library`].
//!
//! The declarations below are transcribed from libinput's `libinput.h`, which carries the MIT
//! Licence. They describe the interface. No code was copied or adapted, and nothing is vendored.
//!
//! # Symbol resolution
//!
//! A symbol is an address inside a mapping. Closing the shared object takes the mapping away, so a
//! call made afterwards jumps into memory the process no longer has. [`Library`] holds the open
//! handle beside the addresses and is the only way to reach them, so the mapping stands for as long
//! as it lives.
//!
//! The rows below are one interface: a context that opens is a context every call is made on, and
//! a version that has the scroll of libinput 1.19 has the rest. A library that misses one symbol
//! therefore fails to load, and [`Error::Symbol`] names the symbol. A libinput too old to be read
//! here says so once, at the load, rather than at the first scroll.

// The table below is libinput's interface rather than a list of this crate's call sites. Every row
// is a symbol the header declares, every row is resolved together with the rest by `Library::load`,
// and every row is checked against the real library by the test at the foot of this file. Some rows
// are called from nowhere yet. Without this allow, declaring the interface in one piece would fail
// `-D warnings` until the last caller was written.
#![allow(dead_code)]

use std::ffi::{OsStr, c_char, c_double, c_int, c_uint, c_void};
use std::fmt;

use crate::error::{Error, Result};

/// The sonames [`Library::load`] tries, in order.
///
/// The versioned name is the one a distribution installs beside a program. The bare name is a
/// development link and is present where the headers are, so it is tried second: a machine with
/// both has one library, and a machine with only the bare name is one somebody builds on.
pub const SONAMES: [&str; 2] = ["libinput.so.10", "libinput.so"];

/// libinput's `struct libinput`.
///
/// The type is never read. A pointer to one is all that crosses, so the body is zero-length and
/// exists to keep that pointer apart from every other pointer in the type system.
#[repr(C)]
pub(crate) struct Libinput {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// libinput's `struct libinput_device`.
#[repr(C)]
pub(crate) struct LibinputDevice {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// libinput's `struct libinput_event`.
#[repr(C)]
pub(crate) struct LibinputEvent {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// libinput's `struct libinput_event_keyboard`.
///
/// One event read as a keyboard event. It is a view of the event rather than a value of its own,
/// so it lives exactly as long as the event does.
#[repr(C)]
pub(crate) struct LibinputKeyboardEvent {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// libinput's `struct libinput_event_pointer`.
///
/// One event read as a pointer event, with the same lifetime as [`LibinputKeyboardEvent`].
#[repr(C)]
pub(crate) struct LibinputPointerEvent {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// libinput's `open_restricted`.
///
/// The caller opens the path and answers the descriptor. A refusal is the negative `errno`, so
/// libinput can tell a device this process may not have from one that is not there.
///
/// The flags are what libinput would open with, and a caller may answer with fewer: libinput asks
/// for `O_RDWR | O_NONBLOCK | O_CLOEXEC` here, and it accepts a read-only descriptor. Such a
/// descriptor gives up the writes — the keyboard lights, and anything else libinput sets on the
/// device.
pub(crate) type OpenRestricted = unsafe extern "C" fn(*const c_char, c_int, *mut c_void) -> c_int;

/// libinput's `close_restricted`.
///
/// The descriptor goes back to whoever opened it. Closing it is that caller's decision, so a device
/// opened through a session daemon can be handed back to the daemon.
pub(crate) type CloseRestricted = unsafe extern "C" fn(c_int, *mut c_void);

/// libinput's `struct libinput_interface`.
///
/// Both fields are required.
///
/// # Lifetime
///
/// `libinput_path_create_context` takes the interface by pointer and stores it. Every device that
/// is opened, and every device that is opened *again* after a resume, is opened through that
/// pointer. The interface therefore has to stay in place, and stay valid, until the context is
/// freed. A `static` satisfies that.
#[repr(C)]
pub(crate) struct Interface {
    /// Open the path, or answer the negative `errno`.
    pub(crate) open_restricted: OpenRestricted,
    /// Take the descriptor back.
    pub(crate) close_restricted: CloseRestricted,
}

/// libinput's `libinput_log_handler`.
///
/// The last argument is a `va_list`, which crosses as a pointer on every target this runs on. A
/// handler that wants the message formats it with the C library's own `vsnprintf`, because the
/// format string and its arguments are C's and Rust cannot read them.
pub(crate) type LogHandler =
    unsafe extern "C" fn(*mut Libinput, c_uint, *const c_char, *mut c_void);

/// Declares the symbol table.
///
/// Each row names a field, the symbol it is resolved from, and the signature it is called with, so
/// a field and the C name behind it cannot drift apart. A C enum crosses as `c_uint`: an enum whose
/// values fit in an `int` is passed in one register, and every one of libinput's does.
macro_rules! symbols {
    (
        $(#[$table:meta])*
        $name:ident {
            $(
                $(#[$row:meta])*
                $field:ident: $symbol:literal => fn($($argument:ty),* $(,)?) $(-> $result:ty)?;
            )+
        }
    ) => {
        $(#[$table])*
        #[derive(Clone, Copy)]
        pub(crate) struct $name {
            $(
                #[doc = concat!("libinput's `", $symbol, "`.")]
                $(#[$row])*
                pub(crate) $field: unsafe extern "C" fn($($argument),*) $(-> $result)?,
            )+
        }

        /// Reports that the table resolved, without listing its addresses.
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }

        impl $name {
            /// Resolves every symbol out of an open shared object, or names the first one that is
            /// missing.
            fn resolve(handle: &libloading::Library) -> Result<Self> {
                Ok(Self { $($field: resolve(handle, $symbol)?,)+ })
            }

            /// Every row, as the symbol it names beside the address that came back, in the order
            /// the rows are declared.
            ///
            /// This exists for the test that reads the table as data. Two rows that resolved to
            /// one address are two rows naming one symbol. That resolves, and it is still wrong.
            #[cfg(test)]
            fn rows(&self) -> Vec<(&'static str, usize)> {
                vec![$(($symbol, self.$field as usize),)+]
            }
        }
    };
}

symbols! {
    /// Every symbol this crate calls.
    ///
    /// libinput reports a failure in the return value — a null pointer, or a negative number — and
    /// puts its reason in its own log rather than in `errno`. So each row below that can fail names
    /// what the failure looks like, and the log is where a reason comes from.
    Symbols {
        // --- the context ------------------------------------------------------------------

        /// Makes a context that reads the devices its caller adds by path, and answers null when
        /// one could not be made.
        ///
        /// libinput's other backend takes a `struct udev *` and finds the devices itself. The path
        /// backend leaves that walk to the caller, so this crate opens no libudev.
        ///
        /// The interface is read through for as long as the context lives, so what is passed here
        /// has to outlive it. See [`Interface`].
        path_create_context: "libinput_path_create_context"
            => fn(*const Interface, *mut c_void) -> *mut Libinput;
        /// Adds one device by the path of its node, and answers null when it could not be added.
        ///
        /// The device is opened through `open_restricted`, so a node the caller refuses arrives
        /// here as null. The device is also reported through a `DEVICE_ADDED` event on the next
        /// dispatch. A resume reports every device it opens again the same way.
        ///
        /// Answers null for a path that is not an evdev node. A path that is not a character
        /// device also draws one `client bug: Invalid path` line on libinput's log; a character
        /// device that is not evdev is opened through `open_restricted` first and then refused
        /// silently.
        ///
        /// **The same path added twice yields two live devices.** libinput deduplicates nothing,
        /// and both of them report everything the device does.
        path_add_device: "libinput_path_add_device"
            => fn(*mut Libinput, *const c_char) -> *mut LibinputDevice;
        /// Removes one device the path backend holds. The device is closed through
        /// `close_restricted` and reported through a `DEVICE_REMOVED` event.
        path_remove_device: "libinput_path_remove_device" => fn(*mut LibinputDevice);
        /// Releases one reference to the context, and answers null when that was the last.
        ///
        /// The last one closes every device still held, through `close_restricted`. So whatever
        /// answers that call has to outlive this one.
        unref: "libinput_unref" => fn(*mut Libinput) -> *mut Libinput;
        /// The descriptor to poll for the events `libinput_dispatch` reads. `-1` on failure.
        ///
        /// It is readable while there is something to read. A caller polls it and dispatches, and
        /// never reads it itself.
        get_fd: "libinput_get_fd" => fn(*mut Libinput) -> c_int;
        /// Reads what the devices have reported and turns it into events. `0` on success, and a
        /// negative `errno` on failure.
        ///
        /// This is the call that makes events available. It does not block.
        dispatch: "libinput_dispatch" => fn(*mut Libinput) -> c_int;
        /// Takes the next event off the queue, or answers null when there is none.
        ///
        /// The event belongs to the caller until `libinput_event_destroy`, and every reader of it
        /// is a call below.
        get_event: "libinput_get_event" => fn(*mut Libinput) -> *mut LibinputEvent;
        /// Closes every device and stops watching for more, keeping the context usable.
        ///
        /// Each device is closed through `close_restricted` and reported removed. The paths are
        /// remembered, and `libinput_resume` opens them again.
        suspend: "libinput_suspend" => fn(*mut Libinput);
        /// Opens every remembered path again through `open_restricted`, and reports each device
        /// added. `0` on success, `-1` on failure.
        ///
        /// The devices are new: a path that was one device before a suspend is another device
        /// after the resume, at another address.
        resume: "libinput_resume" => fn(*mut Libinput) -> c_int;
        /// Sets what libinput does with its own log. Null puts the default back, which writes to
        /// standard error.
        ///
        /// The handler is called from inside whichever libinput call produced the message, on that
        /// caller's thread.
        log_set_handler: "libinput_log_set_handler" => fn(*mut Libinput, Option<LogHandler>);
        /// How much libinput reports through its own log. `10` is debug, `20` information and `30`
        /// errors. A message is reported when its priority is at or above this one.
        log_set_priority: "libinput_log_set_priority" => fn(*mut Libinput, c_uint);

        // --- one event --------------------------------------------------------------------

        /// Frees an event taken with `libinput_get_event`.
        ///
        /// Every view of the event — its device, its keyboard reading, its pointer reading — is
        /// invalid afterwards.
        event_destroy: "libinput_event_destroy" => fn(*mut LibinputEvent);
        /// What kind of event this is. `0` is `NONE`, which no event taken off the queue is.
        event_get_type: "libinput_event_get_type" => fn(*mut LibinputEvent) -> c_uint;
        /// The device the event came from.
        ///
        /// The device is borrowed from the event. A caller that keeps it takes a reference with
        /// `libinput_device_ref`.
        event_get_device: "libinput_event_get_device"
            => fn(*mut LibinputEvent) -> *mut LibinputDevice;
        /// The event read as a keyboard event, which is only valid for a keyboard event type.
        event_get_keyboard_event: "libinput_event_get_keyboard_event"
            => fn(*mut LibinputEvent) -> *mut LibinputKeyboardEvent;
        /// The event read as a pointer event, which is only valid for a pointer event type.
        event_get_pointer_event: "libinput_event_get_pointer_event"
            => fn(*mut LibinputEvent) -> *mut LibinputPointerEvent;

        // --- one device -------------------------------------------------------------------

        /// The device's name, as the kernel published it. The string belongs to the device.
        device_get_name: "libinput_device_get_name" => fn(*mut LibinputDevice) -> *const c_char;
        /// The device's name in `/sys/class/input`, such as `event4`. The string belongs to the
        /// device.
        device_get_sysname: "libinput_device_get_sysname"
            => fn(*mut LibinputDevice) -> *const c_char;
        /// The device's vendor number.
        device_get_id_vendor: "libinput_device_get_id_vendor" => fn(*mut LibinputDevice) -> c_uint;
        /// The device's product number.
        device_get_id_product: "libinput_device_get_id_product"
            => fn(*mut LibinputDevice) -> c_uint;
        /// Whether the device can do one of the things libinput classifies devices by: `0` a
        /// keyboard, `1` a pointer, `2` a touch device, `3` a tablet tool, `4` a tablet pad, `5`
        /// gestures, `6` a switch. Non-zero for yes.
        ///
        /// A device can answer yes to several. One node that carries a key map, two relative axes
        /// and a wheel is a keyboard and a pointer at once, and this machine has such a node.
        device_has_capability: "libinput_device_has_capability"
            => fn(*mut LibinputDevice, c_uint) -> c_int;
        /// Takes a reference to the device, and answers it.
        device_ref: "libinput_device_ref" => fn(*mut LibinputDevice) -> *mut LibinputDevice;
        /// Releases a reference to the device, and answers null when that was the last.
        device_unref: "libinput_device_unref" => fn(*mut LibinputDevice) -> *mut LibinputDevice;
        /// How many fingers this device can tap with, and `0` where tapping is not a thing it does.
        ///
        /// A count above zero is a device that taps, which in practice is a touchpad. The setting
        /// below applies to that device alone.
        device_config_tap_get_finger_count: "libinput_device_config_tap_get_finger_count"
            => fn(*mut LibinputDevice) -> c_int;
        /// Turns tap-to-click on (`1`) or off (`0`). `0` on success, and non-zero where the device
        /// does not have the setting or the value is not one it takes.
        ///
        /// libinput's own default is off.
        device_config_tap_set_enabled: "libinput_device_config_tap_set_enabled"
            => fn(*mut LibinputDevice, c_uint) -> c_uint;

        // --- a key ------------------------------------------------------------------------

        /// Which key moved, as the kernel's own code for it. A layout is asked about this code.
        keyboard_get_key: "libinput_event_keyboard_get_key"
            => fn(*mut LibinputKeyboardEvent) -> u32;
        /// Which way it moved: `0` released, `1` pressed. libinput reports no repeats, because a
        /// repeat is a decision about how long a person has held a key.
        keyboard_get_key_state: "libinput_event_keyboard_get_key_state"
            => fn(*mut LibinputKeyboardEvent) -> c_uint;
        /// When it moved, in microseconds on the same monotonic clock the kernel stamps its own
        /// events with.
        keyboard_get_time_usec: "libinput_event_keyboard_get_time_usec"
            => fn(*mut LibinputKeyboardEvent) -> u64;

        // --- a pointer --------------------------------------------------------------------

        /// When it happened, in microseconds on the monotonic clock.
        pointer_get_time_usec: "libinput_event_pointer_get_time_usec"
            => fn(*mut LibinputPointerEvent) -> u64;
        /// How far the pointer moved across, **after** libinput's acceleration.
        ///
        /// This number makes a slow drag and a fast flick over the same distance move the pointer
        /// by different amounts.
        pointer_get_dx: "libinput_event_pointer_get_dx"
            => fn(*mut LibinputPointerEvent) -> c_double;
        /// How far the pointer moved down, after acceleration.
        pointer_get_dy: "libinput_event_pointer_get_dy"
            => fn(*mut LibinputPointerEvent) -> c_double;
        /// Where an absolute device says it is across, scaled into a width the caller names. A
        /// width of one answers the position as a fraction of the device's own range.
        pointer_get_absolute_x_transformed: "libinput_event_pointer_get_absolute_x_transformed"
            => fn(*mut LibinputPointerEvent, u32) -> c_double;
        /// Where an absolute device says it is down, scaled into a height the caller names.
        pointer_get_absolute_y_transformed: "libinput_event_pointer_get_absolute_y_transformed"
            => fn(*mut LibinputPointerEvent, u32) -> c_double;
        /// Which button changed, as the kernel's own code for it.
        pointer_get_button: "libinput_event_pointer_get_button"
            => fn(*mut LibinputPointerEvent) -> u32;
        /// Which way it changed: `0` released, `1` pressed.
        pointer_get_button_state: "libinput_event_pointer_get_button_state"
            => fn(*mut LibinputPointerEvent) -> c_uint;
        /// Whether this scroll event carries the axis: `0` vertical, `1` horizontal. Non-zero for
        /// yes.
        ///
        /// An axis that is absent is different from one that scrolled by nothing: the second is how
        /// libinput reports that a finger stopped.
        pointer_has_axis: "libinput_event_pointer_has_axis"
            => fn(*mut LibinputPointerEvent, c_uint) -> c_int;
        /// How far this scroll went along the axis, in the unit its source measures in.
        ///
        /// A finger and a continuous source measure in pixels. A wheel measures in detents, and is
        /// read through the row below instead.
        pointer_get_scroll_value: "libinput_event_pointer_get_scroll_value"
            => fn(*mut LibinputPointerEvent, c_uint) -> c_double;
        /// How far a wheel turned, in one hundred and twentieths of a detent.
        ///
        /// This is the unit the kernel's own high-resolution wheel axis counts in, and it is the
        /// one a free-spinning wheel needs: such a wheel reports fine movement continuously and a
        /// whole detent only when it has accumulated one.
        pointer_get_scroll_value_v120: "libinput_event_pointer_get_scroll_value_v120"
            => fn(*mut LibinputPointerEvent, c_uint) -> c_double;
    }
}

/// libinput, open and ready to be called.
pub struct Library {
    /// The open shared object. Every address in `symbols` points into it, so the mapping stands
    /// for as long as this field lives.
    handle: libloading::Library,
    /// The addresses this crate calls.
    symbols: Symbols,
}

impl Library {
    /// Opens libinput, trying each of [`SONAMES`] in turn.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Library`] when no soname could be opened, which is the ordinary answer on a
    /// machine that has no libinput, and [`Error::Symbol`] when one opened and does not carry the
    /// whole table.
    pub fn load() -> Result<Self> {
        let mut reason = String::new();
        for soname in SONAMES {
            match dlopen(OsStr::new(soname)) {
                Ok(handle) => return Self::over(handle),
                Err(why) => reason = why,
            }
        }
        Err(Error::Library {
            tried: SONAMES.iter().map(|name| (*name).to_owned()).collect(),
            reason,
        })
    }

    /// Opens one named shared object and reads libinput's symbols out of it.
    ///
    /// A caller pins one build of the library this way. The tests cover the absence paths with it:
    /// a name that is certainly not on the machine answers [`Error::Library`] on every machine, and
    /// a shared object that is certainly not libinput answers [`Error::Symbol`].
    ///
    /// ```
    /// use zgui_libinput::{Error, Library};
    ///
    /// let error = Library::load_from("libzgui-there-is-no-such-library.so.0")
    ///     .expect_err("no machine carries this name");
    ///
    /// assert!(matches!(error, Error::Library { .. }));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Library`] when the name cannot be opened, and [`Error::Symbol`] when it
    /// opens and does not carry the whole table.
    pub fn load_from(soname: impl AsRef<OsStr>) -> Result<Self> {
        let soname = soname.as_ref();
        let handle = dlopen(soname).map_err(|reason| Error::Library {
            tried: vec![soname.to_string_lossy().into_owned()],
            reason,
        })?;
        Self::over(handle)
    }

    /// Returns the addresses, borrowed from the library they point into.
    pub(crate) fn symbols(&self) -> &Symbols {
        &self.symbols
    }

    /// Resolves every symbol out of an open shared object.
    ///
    /// The addresses are taken before the handle is moved into the structure. Moving a
    /// `libloading::Library` moves the value the loader gave back and leaves the mapping where it
    /// is, so an address taken beforehand still points at the same code.
    fn over(handle: libloading::Library) -> Result<Self> {
        let symbols = Symbols::resolve(&handle)?;
        Ok(Self { handle, symbols })
    }
}

/// Reports that the library is open, without listing its addresses.
impl fmt::Debug for Library {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Library")
            .field("handle", &self.handle)
            .finish_non_exhaustive()
    }
}

/// Opens one shared object, or returns what the loader said.
fn dlopen(soname: &OsStr) -> std::result::Result<libloading::Library, String> {
    // SAFETY: opening a shared object runs the initialisers of everything the loader brings in —
    // the named object and its whole dependency closure. libinput does no work of its own at load
    // time and reads nothing until a context is made. Below it sit the C library, libudev,
    // libevdev, mtdev, libwacom and the Lua interpreter its quirks database uses; `readelf -d`
    // reports that closure here. Every compositor on this machine already loads the same closure at
    // start-up, and its initialisers run there first. The name resolves to whatever the loader
    // decides, and that is the decision it makes for those programs as well.
    unsafe { libloading::Library::new(soname) }.map_err(|error| error.to_string())
}

/// Resolves one symbol, and names it when the library does not have it.
///
/// The address is asked for as an `Option<T>`, so a null answer arrives as `None` rather than as a
/// function pointer nothing can call. `dlsym` reports a missing symbol through `dlerror` on most
/// libraries and answers null without setting it on some, and a shared object can also define a
/// symbol *at* address zero. An `unsafe extern "C" fn` built out of such an address is an invalid
/// value for the type, and that is undefined the moment it exists.
fn resolve<T: Copy>(handle: &libloading::Library, name: &'static str) -> Result<T> {
    // SAFETY: `T` is the signature written beside this symbol in the table above, taken from
    // libinput's header. `Option<T>` has the same layout as `T` for a function pointer, so the null
    // case arrives as `None` and is refused below rather than materialised. The address is copied
    // out of the borrow and kept beside the handle it came from, so it stays valid for as long as
    // anything can call it.
    let symbol: libloading::Symbol<'_, Option<T>> = unsafe { handle.get(name.as_bytes()) }
        .map_err(|error| Error::Symbol {
            name,
            reason: error.to_string(),
        })?;
    (*symbol).ok_or_else(|| Error::Symbol {
        name,
        reason: "the library resolved this symbol to a null address".to_owned(),
    })
}

/// The two names libinput is installed under, written out.
///
/// Held apart from [`SONAMES`] on purpose. The tests check that constant against this one, and ask
/// the machine about this one, so a wrong `SONAMES` cannot report that the machine has no libinput
/// and skip its own test.
#[cfg(test)]
pub(crate) const INSTALLED_AS: [&str; 2] = ["libinput.so.10", "libinput.so"];

/// Returns `true` when one soname opens on this machine.
///
/// The loader is asked directly, around [`Library`]. This is the precondition of the tests that
/// need a real libinput, and a test that asked the subject whether its own precondition holds would
/// skip itself exactly when the subject broke.
#[cfg(test)]
pub(crate) fn is_on_this_machine(soname: &str) -> bool {
    // SAFETY: the call `dlopen` above makes, on the same kind of name, and the handle is dropped
    // straight away. See that function for what makes opening these objects sound.
    unsafe { libloading::Library::new(soname) }.is_ok()
}

#[cfg(test)]
mod tests {
    //! What the loader answers, absent and present.
    //!
    //! The absence paths are the whole reason the library is opened at run time, and they must hold
    //! where libinput *is* installed. So one test asks for a name nothing has, and another asks a
    //! shared object every glibc machine has for symbols it cannot carry. The presence path checks
    //! the thirty-eight hand-transcribed symbol names against the real library.
    //!
    //! A test that can skip settles that for itself, through [`is_on_this_machine`]. Reading the
    //! decision out of the answer the code under test gave would send every regression into the
    //! skip arm, where the suite passes over an assertion nobody makes.

    use super::*;

    /// How many symbols the table has.
    ///
    /// Written out for the same reason as the sonames: a count taken from the table agrees with
    /// the table whatever the table says, including a row that was dropped in a merge.
    const ROWS: usize = 38;

    #[test]
    fn a_soname_nothing_has_is_an_error_rather_than_a_panic() {
        let error = Library::load_from("libzgui-there-is-no-such-library.so.0")
            .expect_err("a name nothing has cannot be opened");

        match error {
            Error::Library { tried, reason } => {
                assert_eq!(tried, ["libzgui-there-is-no-such-library.so.0"]);
                assert!(!reason.is_empty(), "the loader says why");
            }
            other => panic!("an absent library is reported as one: {other}"),
        }
    }

    #[test]
    fn the_sonames_are_the_versioned_name_and_then_the_bare_one() {
        // Written out rather than derived from the constant. An expectation built from `SONAMES`
        // holds for whatever `SONAMES` happens to say, including a name with a typo in it, one
        // name where there should be two, and the two in the wrong order.
        assert_eq!(SONAMES, INSTALLED_AS);
    }

    #[test]
    fn a_shared_object_that_is_not_libinput_names_the_symbol_it_lacks() {
        // The C library is on every machine this crate runs on and carries none of the thirty-eight
        // symbols, so this reaches the second absence path wherever the first one cannot: a library
        // that opens and answers nothing.
        if !is_on_this_machine("libc.so.6") {
            eprintln!(
                "a_shared_object_that_is_not_libinput_names_the_symbol_it_lacks: this machine has \
                 no `libc.so.6` to open, so the missing-symbol path was not covered. Run the suite \
                 on a glibc machine, or from `nix develop`, to cover it."
            );
            return;
        }

        let error = Library::load_from("libc.so.6").expect_err("the C library is not libinput");

        match error {
            Error::Symbol { name, reason } => {
                assert!(
                    name.starts_with("libinput_"),
                    "the missing symbol is named: {name}"
                );
                assert!(!reason.is_empty(), "and the loader says why");
            }
            other => panic!("a library that opens without the table is reported as one: {other}"),
        }
    }

    #[test]
    fn the_whole_table_resolves_out_of_the_real_libinput() {
        // The guard over the thirty-eight symbol names. Each is transcribed from a C header by
        // hand, and a misspelled one resolves nowhere, so it fails here rather than at the first
        // call.
        if !INSTALLED_AS.into_iter().any(is_on_this_machine) {
            eprintln!(
                "the_whole_table_resolves_out_of_the_real_libinput: this machine has no libinput, \
                 so the thirty-eight symbol names were checked against nothing. Install libinput, \
                 or run the suite from `nix develop`, which puts `libinput.so.10` on the library \
                 path."
            );
            return;
        }

        let library = Library::load()
            .unwrap_or_else(|error| panic!("libinput is on this machine, so it loads: {error}"));

        // `Symbols::resolve` stops at the first name it cannot find, so a table that exists at all
        // is a table that resolved whole.
        let rows = library.symbols().rows();
        assert_eq!(rows.len(), ROWS, "the table is the whole interface");

        // A row copied from the one above it and left naming that symbol resolves, and gives one
        // address twice. Resolution alone reports nothing about that.
        for (index, (name, address)) in rows.iter().enumerate() {
            assert!(name.starts_with("libinput_"), "{name} is one of libinput's");
            for (other, other_address) in &rows[index + 1..] {
                assert_ne!(
                    address, other_address,
                    "`{name}` and `{other}` resolved to one address, so a row names the wrong \
                     symbol"
                );
            }
        }
    }
}
