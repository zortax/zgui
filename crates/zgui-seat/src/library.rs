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
//! handle beside the addresses, and [`Library::symbols`] lends them out for as long as it lives.
//!
//! # The symbol table
//!
//! libseat has no optional interface: a seat that opens is a seat every call below is made on. So a
//! library that misses one symbol fails to load, and [`Error::Symbol`] names the symbol.

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
pub struct Libseat {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// One of the two calls a [`SeatListener`] carries.
///
/// The first argument is the seat the change is about. The second is the `userdata` that was given
/// to `libseat_open_seat`.
pub type SeatEvent = unsafe extern "C" fn(*mut Libseat, *mut c_void);

/// libseat's `struct libseat_seat_listener`.
///
/// A seat reports what happened to it through these two calls, and libseat makes them from inside
/// `libseat_open_seat` and `libseat_dispatch`.
#[repr(C)]
pub struct SeatListener {
    /// The session holds its devices, and they can be opened and used.
    pub enable_seat: SeatEvent,
    /// Another session is taking the devices. The seat answers with `libseat_disable_seat`.
    pub disable_seat: SeatEvent,
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
        pub struct $name {
            $(
                #[doc = concat!("libseat's `", $symbol, "`.")]
                $(#[$row])*
                pub $field: unsafe extern "C" fn($($argument),*) $(-> $result)?,
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
        }
    };
}

symbols! {
    /// Every symbol this crate calls.
    ///
    /// Each field carries the answer its call gives on failure, because libseat reports failure in
    /// the return value and says why in `errno`.
    Symbols {
        /// How much libseat reports through its own log. `0` is silent, `1` errors, `2`
        /// information and `3` debug.
        set_log_level: "libseat_set_log_level" => fn(c_uint);
        /// Opens the seat this session is on, and answers null when no backend opened one.
        ///
        /// The first `enable_seat` arrives inside this call, so whatever the listener writes into
        /// exists before it is made.
        open_seat: "libseat_open_seat" => fn(*mut SeatListener, *mut c_void) -> *mut Libseat;
        /// Closes the seat and frees it. `-1` on failure, with `errno` set.
        close_seat: "libseat_close_seat" => fn(*mut Libseat) -> c_int;
        /// Acknowledges a disable. `-1` on failure, with `errno` set.
        disable_seat: "libseat_disable_seat" => fn(*mut Libseat) -> c_int;
        /// The seat's name, which belongs to the seat. Null on failure, with `errno` set.
        seat_name: "libseat_seat_name" => fn(*mut Libseat) -> *const c_char;
        /// Opens a device and writes its descriptor through the pointer.
        ///
        /// The answer is the device id, which `libseat_close_device` takes. `-1` on failure, with
        /// `errno` set.
        open_device: "libseat_open_device" => fn(*mut Libseat, *const c_char, *mut c_int) -> c_int;
        /// Closes a device by the id it was opened with. `-1` on failure, with `errno` set.
        close_device: "libseat_close_device" => fn(*mut Libseat, c_int) -> c_int;
        /// Asks the session daemon for another terminal. `-1` on failure, with `errno` set.
        switch_session: "libseat_switch_session" => fn(*mut Libseat, c_int) -> c_int;
        /// The descriptor to wait on for the events `libseat_dispatch` runs. `-1` on failure, with
        /// `errno` set.
        get_fd: "libseat_get_fd" => fn(*mut Libseat) -> c_int;
        /// Makes the listener calls that are due, waiting up to a number of milliseconds for
        /// something to arrive. `-1` on failure, with `errno` set.
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
    ///
    /// The borrow is the guarantee: a caller holds the mapping open for as long as it can reach an
    /// address inside it.
    pub fn symbols(&self) -> &Symbols {
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
    // SAFETY: opening a shared object runs its initialisers, so this is only sound for an object
    // whose initialisers are sound. libseat has none of its own: it makes a connection when a seat
    // is asked for, and does nothing at load time. What the name resolves to is the loader's
    // decision, which is the same decision every program that links it makes.
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
    //! The absence paths.
    //!
    //! They are the whole reason the library is opened at run time, and they must hold where
    //! libseat *is* installed, so one asks for a name nothing has and another asks a shared object
    //! every glibc machine has for symbols it cannot have.

    use super::*;

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
    fn an_absent_library_says_which_sonames_it_looked_for() {
        // A person reading the message has to be able to check the machine against it, so the list
        // is the whole list rather than the last name tried.
        let error = Error::Library {
            tried: SONAMES.iter().map(|name| (*name).to_owned()).collect(),
            reason: "cannot open shared object file".to_owned(),
        };

        let message = error.to_string();
        for soname in SONAMES {
            assert!(message.contains(soname), "{message} names {soname}");
        }
    }

    #[test]
    fn a_shared_object_that_is_not_libseat_names_the_symbol_it_lacks() {
        // The C library is on every machine this crate runs on and carries none of the ten symbols,
        // so this reaches the second absence path wherever the first one cannot: a library that
        // opens and answers nothing. A machine without `libc.so.6` gets the first path again, which
        // is still an error rather than a panic.
        let error = Library::load_from("libc.so.6").expect_err("the C library is not libseat");

        match error {
            Error::Symbol { name, reason } => {
                assert!(
                    name.starts_with("libseat_"),
                    "the missing symbol is named: {name}"
                );
                assert!(!reason.is_empty(), "and the loader says why");
            }
            Error::Library { .. } => {
                eprintln!(
                    "a_shared_object_that_is_not_libseat_names_the_symbol_it_lacks: this machine \
                     has no `libc.so.6` to ask, so nothing was asserted"
                );
            }
        }
    }
}
