//! libseat, opened at run time, and the symbols taken out of it.
//!
//! Nothing in this crate is linked against libseat. The shared object is opened with `dlopen`,
//! every symbol is resolved once, and the addresses are kept beside the open handle. A build
//! therefore needs neither the library nor a session daemon, and a machine without either answers
//! [`Library::load`] with [`Error::Library`].
//!
//! The declarations below are transcribed from libseat's `libseat.h`, which carries the MIT
//! Licence. They describe the interface. No code was copied or adapted, and nothing is vendored.
//!
//! # The library handle
//!
//! A symbol is an address inside a mapping. Closing the shared object takes the mapping away, so a
//! call made afterwards jumps into memory the process no longer has. [`Library`] holds the open
//! handle beside the addresses and is the only way to reach them, so the mapping stands for as long
//! as it lives.
//!
//! # The symbol table
//!
//! libseat has no optional interface: a seat that opens is a seat every call below is made on. So a
//! library that misses one symbol fails to load, and [`Error::Symbol`] names the symbol.

// The table below is libseat's interface rather than a list of this crate's call sites. Every row
// is a symbol the header declares, every row is resolved together with the rest by `Library::load`
// and checked against the real library by the test at the foot of this file, and no row is called
// yet. Without this, declaring the interface in one piece would fail `-D warnings` until the last
// caller was written.
#![allow(dead_code)]

use std::ffi::{OsStr, c_char, c_int, c_uint, c_void};
use std::fmt;

use crate::error::{Error, Result};

/// The sonames [`Library::load`] tries, in order.
///
/// The versioned name is the one a distribution installs beside a program. The bare name is a
/// development link and is present where the headers are, so it is tried second: a machine with
/// both has one library, and a machine with only the bare name is one somebody builds on.
pub const SONAMES: [&str; 2] = ["libseat.so.1", "libseat.so"];

/// libseat's `struct libseat`.
///
/// The type is never read. A pointer to one is all that crosses, so the body is zero-length and
/// exists to keep that pointer apart from every other pointer in the type system.
#[repr(C)]
pub(crate) struct Libseat {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// One of the two calls a [`SeatListener`] carries.
///
/// The first argument is the seat the change is about. The second is the `userdata` that was given
/// to `libseat_open_seat`.
pub(crate) type SeatEvent = unsafe extern "C" fn(*mut Libseat, *mut c_void);

/// libseat's `struct libseat_seat_listener`.
///
/// A seat reports what happened to it through these two calls. libseat makes them from inside
/// `libseat_open_seat` and from inside `libseat_dispatch`, and which one carries the first event is
/// the backend's decision: seatd and builtin can enable a seat while it is being opened, and logind
/// holds the first event for the first dispatch.
///
/// Both fields are required. libseat refuses a listener with a null field, and answers `EINVAL`.
///
/// `libseat_open_seat` takes the listener by pointer and stores it. Every backend holds that
/// pointer in the seat and reads through it on each event, so the listener has to stay in place,
/// and stay valid, until the seat is closed. A listener built on the stack of the function that
/// opens the seat leaves a dangling pointer at the first terminal switch. Give libseat an address
/// that outlives the seat, and leave the value at that address where it is.
#[repr(C)]
pub(crate) struct SeatListener {
    /// The session holds its devices, and they can be used again. Every device is opened again as
    /// well, because a descriptor from before can have been blocked or revoked.
    pub(crate) enable_seat: SeatEvent,
    /// Another session is taking the devices. The seat answers with `libseat_disable_seat`, and
    /// soon: a seat that is slow to answer has its devices taken from it.
    pub(crate) disable_seat: SeatEvent,
}

/// Declares the symbol table.
///
/// Each row names a field, the symbol it is resolved from, and the signature it is called with, so
/// a field and the C name behind it cannot drift apart. A C enum crosses as `c_uint`: an enum whose
/// values fit in an `int` is passed in one register, and libseat's log level does.
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
                #[doc = concat!("libseat's `", $symbol, "`.")]
                $(#[$row])*
                pub(crate) $field: unsafe extern "C" fn($($argument),*) $(-> $result)?,
            )+
        }

        /// Says that the table resolved, without listing its addresses.
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
            /// one address are two rows naming one symbol, which resolves and is still wrong.
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
    /// libseat puts a failure in the return value and the reason in `errno`, so each row below that
    /// can fail names the answer it fails with. `libseat_set_log_level` returns nothing, and is the
    /// one row with no failure to name.
    Symbols {
        /// How much libseat reports through its own log. `0` is silent, `1` errors, `2`
        /// information and `3` debug. A message is processed when its level is at or below this
        /// one. The call returns nothing and always takes effect.
        set_log_level: "libseat_set_log_level" => fn(c_uint);
        /// Opens the seat this session is on, and answers null with `errno` set when no backend
        /// opened one.
        ///
        /// A handle means that a backend accepted the call. The seat becomes usable when
        /// `enable_seat` arrives, and a backend can hand back a live handle for a seat that never
        /// enables — the builtin backend with no terminal to take does exactly that. So a caller
        /// waits for that call, on a bound, and treats the bound running out as a seat it did not
        /// get.
        ///
        /// `LIBSEAT_BACKEND` names the backend to use. Without it libseat tries each backend it
        /// was built with, in order, and takes the first that opens a seat.
        ///
        /// The listener is read through for as long as the seat is open, so what is passed here
        /// has to outlive it. See [`SeatListener`].
        open_seat: "libseat_open_seat" => fn(*const SeatListener, *mut c_void) -> *mut Libseat;
        /// Closes the seat and frees it. `0` on success, `-1` on failure with `errno` set.
        close_seat: "libseat_close_seat" => fn(*mut Libseat) -> c_int;
        /// Acknowledges a disable, which is required shortly after `disable_seat` arrives. The
        /// devices stay unused, and every request on the seat fails, until `enable_seat` arrives
        /// again. `0` on success, `-1` on failure with `errno` set.
        disable_seat: "libseat_disable_seat" => fn(*mut Libseat) -> c_int;
        /// The seat's name. The string belongs to the libseat instance and stays valid for as long
        /// as the seat is open, and it is never written to, so a caller that keeps it copies it.
        seat_name: "libseat_seat_name" => fn(*mut Libseat) -> *const c_char;
        /// Opens a device and writes its descriptor through the pointer.
        ///
        /// This succeeds while the seat is active, and for the device types the backend permits,
        /// which are DRM and evdev. An open device can still be revoked, such as where a session
        /// switch is being forced.
        ///
        /// The answer is the device id, which `libseat_close_device` takes. `-1` on failure, with
        /// `errno` set.
        open_device: "libseat_open_device" => fn(*mut Libseat, *const c_char, *mut c_int) -> c_int;
        /// Closes a device by the id it was opened with. `0` on success, `-1` on failure with
        /// `errno` set.
        close_device: "libseat_close_device" => fn(*mut Libseat, c_int) -> c_int;
        /// Asks the seat to switch to another session. A seat bound to a terminal numbers its
        /// sessions the way the terminals are numbered, so this is how a terminal is asked for.
        ///
        /// The answer says the request went out, and a switch can still fail to happen. A caller
        /// carries on as though the session is unchanged, and learns that it moved from
        /// `disable_seat`. `0` on success, `-1` on failure with `errno` set.
        switch_session: "libseat_switch_session" => fn(*mut Libseat, c_int) -> c_int;
        /// The descriptor to poll for the events `libseat_dispatch` runs. `-1` on failure, with
        /// `errno` set.
        get_fd: "libseat_get_fd" => fn(*mut Libseat) -> c_int;
        /// Reads what has arrived on the connection and makes the listener calls that are due.
        ///
        /// The second argument bounds the wait when nothing has arrived: `0` waits for nothing, a
        /// positive number is the longest wait in milliseconds, and `-1` lets libseat wait for as
        /// long as it takes. A program stopped inside this call answers nothing while it holds the
        /// terminal, so a caller that has to stay answerable gives a bound.
        ///
        /// The answer counts the internal messages processed, and is `0` where there were none.
        /// `-1` on failure, with `errno` set.
        dispatch: "libseat_dispatch" => fn(*mut Libseat, c_int) -> c_int;
    }
}

/// libseat, open and ready to be called.
///
/// ```
/// use zgui_seat::{Error, Library};
///
/// match Library::load() {
///     Ok(library) => assert!(format!("{library:?}").starts_with("Library")),
///     Err(error) => assert!(matches!(
///         error,
///         Error::Library { .. } | Error::Symbol { .. }
///     )),
/// }
/// ```
pub struct Library {
    /// The open shared object. Every address in `symbols` points inside it, so this field keeps
    /// them valid, and dropping it leaves them pointing into memory the process no longer has.
    handle: libloading::Library,
    /// The addresses this crate calls.
    symbols: Symbols,
}

impl Library {
    /// Opens libseat, trying each of [`SONAMES`] in turn.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Library`] when no soname could be opened, which is the ordinary answer on a
    /// machine that has no libseat, and [`Error::Symbol`] when one opened and does not carry the
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

    /// Opens one named shared object and reads libseat's symbols out of it.
    ///
    /// This is how a caller pins a build of the library, and it is how the absence paths are
    /// tested: a name that is certainly not on the machine gets [`Error::Library`] on every
    /// machine, and a shared object that is certainly not libseat gets [`Error::Symbol`].
    ///
    /// ```
    /// use zgui_seat::{Error, Library};
    ///
    /// assert!(matches!(
    ///     Library::load_from("libseat.so.999"),
    ///     Err(Error::Library { .. })
    /// ));
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

    /// The addresses, borrowed from the library they point into.
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

/// Says that the library is open, without listing ten addresses.
impl fmt::Debug for Library {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Library")
            .field("handle", &self.handle)
            .finish_non_exhaustive()
    }
}

/// Opens one shared object, or says what the loader said.
fn dlopen(soname: &OsStr) -> std::result::Result<libloading::Library, String> {
    // SAFETY: opening a shared object runs the initialisers of everything the loader brings in —
    // the named object and its whole dependency closure. libseat does no work of its own at load
    // time and makes its connection when a seat is asked for. Below it sits the C library, and
    // libsystemd where libseat was built with the logind backend, as `readelf -d` reports here.
    // That closure is the one every program linked against libseat already loads at start-up, and
    // its initialisers run there first. What the name resolves to is the loader's decision, which
    // is the same decision it makes for those programs.
    unsafe { libloading::Library::new(soname) }.map_err(|error| error.to_string())
}

/// Resolves one symbol, and names it when the library does not have it.
///
/// The address is asked for as an `Option<T>`, so a null answer arrives as `None` rather than as a
/// function pointer nothing can call. `dlsym` reports a missing symbol through `dlerror` on most
/// libraries and answers null without setting it on some, and a shared object can also define a
/// symbol *at* address zero. Building an `unsafe extern "C" fn` out of that is an invalid value for
/// the type — undefined the moment it exists, before anything calls it.
fn resolve<T: Copy>(handle: &libloading::Library, name: &'static str) -> Result<T> {
    // SAFETY: `T` is the signature written beside this symbol in the table above, taken from
    // libseat's header. `Option<T>` has the same layout as `T` for a function pointer, so the null
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

#[cfg(test)]
mod tests {
    //! What the loader answers, absent and present.
    //!
    //! The absence paths are the whole reason the library is opened at run time, and they must hold
    //! where libseat *is* installed, so one asks for a name nothing has and another asks a shared
    //! object every glibc machine has for symbols it cannot have. The presence path checks the ten
    //! hand-transcribed symbol names against the real library.
    //!
    //! A test that can skip settles that for itself, through [`is_on_this_machine`]. Reading the
    //! decision out of the answer the code under test gave would put every regression in the silent
    //! arm, where the suite stays green over an assertion nobody makes.

    use super::*;

    /// The two names libseat is installed under, written out.
    ///
    /// Deliberately apart from [`SONAMES`]. This is what the tests below check that constant
    /// against, and what they ask the machine about, so a wrong `SONAMES` cannot decide that the
    /// machine has no libseat and send its own test into the silent arm.
    const INSTALLED_AS: [&str; 2] = ["libseat.so.1", "libseat.so"];

    /// Returns `true` if this soname opens on this machine, asked of the loader directly.
    ///
    /// This goes around [`Library`] on purpose. It is the precondition of the tests below, and a
    /// test that asked the subject whether its own precondition holds would skip itself exactly
    /// when the subject broke.
    fn is_on_this_machine(soname: &str) -> bool {
        // SAFETY: the call `dlopen` above makes, on the same kind of name, and the handle is
        // dropped straight away. See that function for what makes opening these objects sound.
        unsafe { libloading::Library::new(soname) }.is_ok()
    }

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
    fn a_shared_object_that_is_not_libseat_names_the_symbol_it_lacks() {
        // The C library is on every machine this crate runs on and carries none of the ten symbols,
        // so this reaches the second absence path wherever the first one cannot: a library that
        // opens and answers nothing.
        if !is_on_this_machine("libc.so.6") {
            eprintln!(
                "a_shared_object_that_is_not_libseat_names_the_symbol_it_lacks: this machine has \
                 no `libc.so.6` to open, so the missing-symbol path was not covered. Run the suite \
                 on a glibc machine, or from `nix develop`, to cover it."
            );
            return;
        }

        let error = Library::load_from("libc.so.6").expect_err("the C library is not libseat");

        match error {
            Error::Symbol { name, reason } => {
                assert!(
                    name.starts_with("libseat_"),
                    "the missing symbol is named: {name}"
                );
                assert!(!reason.is_empty(), "and the loader says why");
            }
            other => panic!("a library that opens without the table is reported as one: {other}"),
        }
    }

    #[test]
    fn the_whole_table_resolves_out_of_the_real_libseat() {
        // The guard over the ten symbol names. Each is transcribed from a C header by hand, and a
        // misspelled one resolves nowhere, so it fails here rather than at the first call.
        if !INSTALLED_AS.into_iter().any(is_on_this_machine) {
            eprintln!(
                "the_whole_table_resolves_out_of_the_real_libseat: this machine has no libseat, so \
                 the ten symbol names were checked against nothing. Install seatd, or run the \
                 suite from `nix develop`, which puts `libseat.so.1` on the library path."
            );
            return;
        }

        let library = Library::load()
            .unwrap_or_else(|error| panic!("libseat is on this machine, so it loads: {error}"));

        // `Symbols::resolve` stops at the first name it cannot find, so a table that exists at all
        // is a table that resolved whole.
        let rows = library.symbols().rows();
        assert_eq!(rows.len(), 10, "the table is the whole interface");

        // A row copied from the one above it and left naming that symbol resolves, and gives one
        // address twice. Resolution alone reports nothing about that.
        for (index, (name, address)) in rows.iter().enumerate() {
            assert!(name.starts_with("libseat_"), "{name} is one of libseat's");
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
