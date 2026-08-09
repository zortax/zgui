//! What went wrong.

use std::fmt;

use crate::context::RuleNames;

/// The result of a call into libxkbcommon.
pub type Result<T> = std::result::Result<T, Error>;

/// What a call into libxkbcommon refused with.
///
/// ```
/// use zgui_xkb::Error;
///
/// let error = Error::Name { field: "layout" };
///
/// assert_eq!(error.to_string(), "the layout holds a zero byte");
/// ```
// A case is added together with the code that builds it. `cargo xtask ledger inert` fails a variant
// that is matched on and built nowhere, because such a case reads like a working feature that is
// merely unused.
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// libxkbcommon is not on this machine.
    ///
    /// This is the case the crate is shaped around. The library is opened at run time, so a build
    /// needs neither it nor its data files, and a machine without it hands the caller this value
    /// to read.
    Library {
        /// The sonames that were tried, in order.
        tried: Vec<String>,
        /// What the loader said about the last one.
        reason: String,
    },
    /// The library is here, and it does not carry a symbol this crate calls.
    ///
    /// A libxkbcommon older than the interface this was written against lands here. It is a
    /// separate case from an absent library because the answer is different: a caller can report
    /// which symbol is missing, and a person can read a version out of that.
    Symbol {
        /// The symbol that was asked for.
        name: &'static str,
        /// What the loader said.
        reason: String,
    },
    /// A keymap did not compile.
    ///
    /// libxkbcommon answers with nothing and writes its reason to its own log, so what this
    /// carries is what was asked for. Absent keyboard data and a layout name the rules do not know
    /// both arrive here, and the C interface tells them apart nowhere.
    Keymap {
        /// The names the keymap was asked for.
        names: RuleNames,
    },
    /// There is no compose table for a locale.
    ///
    /// The compose sequences ship apart from the keyboard data, in the X11 locale directory, so a
    /// machine can compile every keymap and hold no compose file at all.
    Compose {
        /// The locale that was asked for.
        locale: String,
    },
    /// A name holds a zero byte and cannot cross to C.
    ///
    /// A C string ends at its first zero, so such a name would arrive cut short and compile a
    /// keymap nobody asked for.
    Name {
        /// Which name it is.
        field: &'static str,
    },
    /// The library refused to build an object and gave no reason.
    ///
    /// A context and a state come back empty when an allocation fails, which is the only failure
    /// either of them has. There is nothing to report but what was asked for.
    Refused {
        /// The call that answered with nothing.
        what: &'static str,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library { tried, reason } => write!(
                f,
                "libxkbcommon is not on this machine; tried {}: {reason}",
                tried.join(", ")
            ),
            Self::Symbol { name, reason } => write!(f, "libxkbcommon has no `{name}`: {reason}"),
            Self::Keymap { names } => write!(f, "no keymap compiles from {names}"),
            Self::Compose { locale } => write!(f, "there is no compose table for `{locale}`"),
            Self::Name { field } => write!(f, "the {field} holds a zero byte"),
            Self::Refused { what } => write!(f, "{what} answered with nothing"),
        }
    }
}

impl std::error::Error for Error {}
