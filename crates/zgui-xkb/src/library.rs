//! Opening libxkbcommon at run time, and the symbols taken out of it.
//!
//! Nothing in this crate is linked against libxkbcommon. The shared object is opened with
//! `dlopen`, every symbol is resolved once, and the addresses are kept beside the open handle. A
//! build therefore needs neither the library nor the keyboard data it reads, and a machine without
//! either answers [`Library::load`] with [`Error::Library`].
//!
//! # The share every handle holds
//!
//! A symbol is an address inside a mapping. Closing the shared object takes the mapping away, so a
//! call made afterwards jumps into memory the process no longer has. Each handle this crate hands
//! out therefore keeps its own [`Arc<Library>`], and the mapping goes when the last of them goes.

use std::ffi::{OsStr, c_char, c_int, c_uint};
use std::fmt;
use std::sync::Arc;

use crate::error::{Error, Result};

/// The sonames [`Library::load`] tries, in order.
///
/// The versioned name is the one a distribution installs beside a program. The bare name is a
/// development link, present where the headers are, so it is tried second.
// A machine with both names has one library, and a machine with only the bare name is one somebody
// builds on.
pub const SONAMES: [&str; 2] = ["libxkbcommon.so.0", "libxkbcommon.so"];

/// libxkbcommon's `xkb_context`.
///
/// The five opaque types below are never read. A pointer to one is all that crosses, so each is a
/// zero-length body that exists to keep the pointers apart in the type system.
#[repr(C)]
pub(crate) struct XkbContext {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// libxkbcommon's `xkb_keymap`.
#[repr(C)]
pub(crate) struct XkbKeymap {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// libxkbcommon's `xkb_state`.
#[repr(C)]
pub(crate) struct XkbState {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// libxkbcommon's `xkb_compose_table`.
#[repr(C)]
pub(crate) struct XkbComposeTable {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// libxkbcommon's `xkb_compose_state`.
#[repr(C)]
pub(crate) struct XkbComposeState {
    /// Nothing. This type is only ever pointed at.
    _opaque: [u8; 0],
}

/// libxkbcommon's `struct xkb_rule_names`.
///
/// Every field is a C string or null, and null means "use the default". The strings belong to the
/// caller and are read during the call that takes this.
#[repr(C)]
pub(crate) struct RuleNamesRaw {
    /// The rules file.
    pub(crate) rules: *const c_char,
    /// The hardware model.
    pub(crate) model: *const c_char,
    /// The layout, or several separated by commas.
    pub(crate) layout: *const c_char,
    /// The variant of each layout.
    pub(crate) variant: *const c_char,
    /// The options, separated by commas.
    pub(crate) options: *const c_char,
}

/// The flag value every entry point in this crate passes: none of them.
pub(crate) const NO_FLAGS: c_uint = 0;

/// Declares the symbol table.
///
/// Each row names a field, the symbol it is resolved from, and the signature it is called with, so
/// a field and the C name behind it cannot drift apart. Every C enum crosses as `c_uint`: an enum
/// whose values fit in an `int` is passed and returned in one register, and every enum here does.
macro_rules! symbols {
    ($($field:ident: $symbol:literal => fn($($argument:ty),* $(,)?) $(-> $result:ty)?;)+) => {
        /// The addresses this crate calls, resolved once when the library is opened.
        pub(crate) struct Symbols {
            $(
                #[doc = concat!("libxkbcommon's `", $symbol, "`.")]
                pub(crate) $field: unsafe extern "C" fn($($argument),*) $(-> $result)?,
            )+
        }

        impl Symbols {
            /// Resolves every symbol out of an open shared object.
            fn resolve(handle: &libloading::Library) -> Result<Self> {
                Ok(Self { $($field: resolve(handle, $symbol)?,)+ })
            }
        }
    };
}

symbols! {
    context_new: "xkb_context_new" => fn(c_uint) -> *mut XkbContext;
    context_unref: "xkb_context_unref" => fn(*mut XkbContext);
    keymap_new_from_names: "xkb_keymap_new_from_names"
        => fn(*mut XkbContext, *const RuleNamesRaw, c_uint) -> *mut XkbKeymap;
    keymap_unref: "xkb_keymap_unref" => fn(*mut XkbKeymap);
    keymap_key_repeats: "xkb_keymap_key_repeats" => fn(*mut XkbKeymap, u32) -> c_int;
    keymap_key_get_syms_by_level: "xkb_keymap_key_get_syms_by_level"
        => fn(*mut XkbKeymap, u32, u32, u32, *mut *const u32) -> c_int;
    state_new: "xkb_state_new" => fn(*mut XkbKeymap) -> *mut XkbState;
    state_unref: "xkb_state_unref" => fn(*mut XkbState);
    state_update_key: "xkb_state_update_key" => fn(*mut XkbState, u32, c_uint) -> c_uint;
    state_key_get_utf8: "xkb_state_key_get_utf8"
        => fn(*mut XkbState, u32, *mut c_char, usize) -> c_int;
    state_key_get_one_sym: "xkb_state_key_get_one_sym" => fn(*mut XkbState, u32) -> u32;
    state_key_get_layout: "xkb_state_key_get_layout" => fn(*mut XkbState, u32) -> u32;
    state_mod_name_is_active: "xkb_state_mod_name_is_active"
        => fn(*mut XkbState, *const c_char, c_uint) -> c_int;
    keysym_get_name: "xkb_keysym_get_name" => fn(u32, *mut c_char, usize) -> c_int;
    compose_table_new_from_locale: "xkb_compose_table_new_from_locale"
        => fn(*mut XkbContext, *const c_char, c_uint) -> *mut XkbComposeTable;
    compose_table_unref: "xkb_compose_table_unref" => fn(*mut XkbComposeTable);
    compose_state_new: "xkb_compose_state_new"
        => fn(*mut XkbComposeTable, c_uint) -> *mut XkbComposeState;
    compose_state_unref: "xkb_compose_state_unref" => fn(*mut XkbComposeState);
    compose_state_feed: "xkb_compose_state_feed" => fn(*mut XkbComposeState, u32) -> c_uint;
    compose_state_get_status: "xkb_compose_state_get_status" => fn(*mut XkbComposeState) -> c_uint;
    compose_state_get_utf8: "xkb_compose_state_get_utf8"
        => fn(*mut XkbComposeState, *mut c_char, usize) -> c_int;
    compose_state_get_one_sym: "xkb_compose_state_get_one_sym" => fn(*mut XkbComposeState) -> u32;
    compose_state_reset: "xkb_compose_state_reset" => fn(*mut XkbComposeState);
}

/// libxkbcommon, open and ready to be called.
///
/// A caller that wants one keyboard asks [`crate::Context::new`] and never names this type. A
/// caller with several keyboards opens the library once with [`Library::load`] and hands the same
/// share to each [`crate::Context`], so one mapping serves them all.
///
/// ```no_run
/// use zgui_xkb::{Context, Library};
///
/// let library = Library::load()?;
/// let first = Context::over(library.clone())?;
/// let second = Context::over(library)?;
/// # Ok::<(), zgui_xkb::Error>(())
/// ```
pub struct Library {
    /// The open shared object. Every address in `symbols` points inside it, so this field keeps
    /// them valid, and dropping it leaves them pointing at memory the process no longer has.
    handle: libloading::Library,
    /// The addresses this crate calls.
    pub(crate) symbols: Symbols,
}

impl Library {
    /// Opens libxkbcommon, trying each of [`SONAMES`] in turn.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Library`] when no soname could be opened, which is the ordinary answer on
    /// a machine that has no libxkbcommon, and [`Error::Symbol`] when one opened and does not
    /// carry a symbol this crate calls.
    pub fn load() -> Result<Arc<Self>> {
        let mut reason = String::new();
        for soname in SONAMES {
            match dlopen(OsStr::new(soname)) {
                Ok(handle) => return Self::over(handle).map(Arc::new),
                Err(why) => reason = why,
            }
        }
        Err(Error::Library {
            tried: SONAMES.iter().map(|name| (*name).to_owned()).collect(),
            reason,
        })
    }

    /// Opens one named shared object and reads libxkbcommon's symbols out of it.
    ///
    /// This is how a caller pins a build of the library, and it is how the absence path is tested:
    /// a name that is certainly not on the machine gets [`Error::Library`] on every machine.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Library`] when the name cannot be opened, and [`Error::Symbol`] when it
    /// opens and does not carry a symbol this crate calls.
    pub fn load_from(soname: impl AsRef<OsStr>) -> Result<Arc<Self>> {
        let soname = soname.as_ref();
        let handle = dlopen(soname).map_err(|reason| Error::Library {
            tried: vec![soname.to_string_lossy().into_owned()],
            reason,
        })?;
        Self::over(handle).map(Arc::new)
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

/// Says that the library is open, without listing twenty-two addresses.
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
    // whose initialisers are sound. libxkbcommon has none of its own: it is a parser and a table
    // lookup over data files, with no constructor, no thread and no signal handler. What the name
    // resolves to is the loader's decision, the same decision every program that links it makes.
    unsafe { libloading::Library::new(soname) }.map_err(|error| error.to_string())
}

/// Resolves one symbol, and names it when the library does not have it.
fn resolve<T: Copy>(handle: &libloading::Library, name: &'static str) -> Result<T> {
    // SAFETY: `T` is the signature written beside this symbol in the table above, taken from
    // `xkbcommon.h`. The address is copied out of the borrow and kept beside the handle it came
    // from, so it stays valid for as long as anything can call it.
    let symbol: libloading::Symbol<'_, T> =
        unsafe { handle.get(name.as_bytes()) }.map_err(|error| Error::Symbol {
            name,
            reason: error.to_string(),
        })?;
    Ok(*symbol)
}

/// Reads a string out of a call that fills a buffer and answers with the length it needed.
///
/// libxkbcommon writes as much as fits, terminates it, and returns the number of bytes the whole
/// string needs without the terminator. A first call with a buffer on the stack covers everything
/// a keyboard produces; a longer answer is asked for again with a buffer that holds it. Skipping
/// the second call truncates a character in the middle of its bytes.
///
/// The answer is `None` when the call reported nothing to write, and when it reported a negative
/// length — `xkb_keysym_get_name` says `-1` for a symbol that has no name.
pub(crate) fn read_text(mut fill: impl FnMut(*mut c_char, usize) -> c_int) -> Option<String> {
    let mut buffer = [0_u8; 64];
    let needed = usize::try_from(fill(buffer.as_mut_ptr().cast(), buffer.len())).ok()?;
    if needed == 0 {
        return None;
    }
    // The terminator is written too, so a string exactly as long as the buffer did not fit.
    if needed < buffer.len() {
        return Some(String::from_utf8_lossy(&buffer[..needed]).into_owned());
    }

    let mut grown = vec![0_u8; needed + 1];
    let written = usize::try_from(fill(grown.as_mut_ptr().cast(), grown.len()))
        .ok()?
        .min(needed);
    Some(String::from_utf8_lossy(&grown[..written]).into_owned())
}

#[cfg(test)]
mod tests {
    //! The absence path, and the buffer pattern.
    //!
    //! Both run on every machine. The first is the whole reason the library is opened at run time,
    //! and it must hold where libxkbcommon *is* installed, so it asks for a name nothing has.

    use super::*;

    #[test]
    fn one_open_library_serves_every_thread() {
        // The mapping and the addresses inside it are the same whichever thread reads them, so a
        // process with two keyboard threads opens the shared object once. A context stays where it
        // was made; it holds raw pointers, and those keep it there.
        const fn crosses<T: Send + Sync>() {}
        crosses::<Library>();
        crosses::<Arc<Library>>();
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
    fn an_absent_library_says_which_sonames_it_looked_for() {
        // A person reading the message has to be able to check the machine against it, so the
        // message names every soname rather than the last one tried.
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
    fn a_string_longer_than_the_first_buffer_is_asked_for_again() {
        // The two-call pattern, with libxkbcommon's own contract written out: write what fits,
        // terminate it, and answer with the length of the whole string.
        let text = "e".repeat(200);
        let mut calls = 0;
        let read = read_text(|buffer, size| {
            calls += 1;
            let bytes = text.as_bytes();
            let fits = bytes.len().min(size.saturating_sub(1));
            // SAFETY: the buffer is `size` bytes and `fits` is below it, so the copy and the
            // terminator after it are inside it.
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), fits);
                buffer.add(fits).write(0);
            }
            c_int::try_from(bytes.len()).expect("two hundred bytes fit in an int")
        });

        assert_eq!(read.as_deref(), Some(text.as_str()));
        assert_eq!(calls, 2, "the first call sized it and the second read it");
    }

    #[test]
    fn a_string_that_fits_is_read_in_one_call() {
        let mut calls = 0;
        let read = read_text(|buffer, _size| {
            calls += 1;
            // SAFETY: the buffer is at least sixty-four bytes, and two are written.
            unsafe {
                buffer.write(b'a' as c_char);
                buffer.add(1).write(0);
            }
            1
        });

        assert_eq!(read.as_deref(), Some("a"));
        assert_eq!(calls, 1);
    }

    #[test]
    fn a_call_with_nothing_to_say_reads_as_nothing() {
        assert_eq!(read_text(|_, _| 0), None, "a key that produces no text");
        assert_eq!(read_text(|_, _| -1), None, "a keysym that has no name");
    }
}
