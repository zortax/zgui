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
    /// A libxkbcommon older than the interface this was written against lands here, and so does a
    /// shared object that is not libxkbcommon at all. It is a separate case from an absent library
    /// because the answer is different: a caller can report which symbol is missing, and a person
    /// can read a version out of that.
    ///
    /// Only the symbols a keyboard cannot work without fail the load. A missing compose interface
    /// or a missing keysym-naming call reaches this error at the one entry point that needs it,
    /// and costs nothing else.
    Symbol {
        /// The symbol that was asked for.
        name: &'static str,
        /// What the loader said, or why the address it gave cannot be used.
        reason: String,
    },
    /// A keymap did not compile.
    ///
    /// libxkbcommon answers with nothing and says why through its log, which this crate captures.
    /// Absent keyboard data and a layout name the rules do not know both arrive here, and only the
    /// reason tells them apart.
    ///
    /// The names are boxed because every other case here is small, and an error that is large
    /// makes every `Result` in the crate large with it.
    Keymap {
        /// The names the keymap was asked for.
        names: Box<RuleNames>,
        /// What libxkbcommon said while it tried, or nothing when it said nothing.
        reason: String,
    },
    /// There is no compose table for a locale.
    ///
    /// The compose sequences ship apart from the keyboard data, in the X11 locale directory, so a
    /// machine can compile every keymap and hold no compose file at all.
    Compose {
        /// The locale that was asked for.
        locale: String,
        /// What libxkbcommon said while it tried, or nothing when it said nothing.
        reason: String,
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
    /// A context, a state and a compose state each answer with nothing on failure and report
    /// nothing about why. An allocation failure is the case this crate expects; whether the loaded
    /// library has others is a question about code this crate never compiles, so the message
    /// claims only what the interface promises.
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
            Self::Keymap { names, reason } if reason.is_empty() => {
                write!(f, "no keymap compiles from {names}")
            }
            Self::Keymap { names, reason } => {
                write!(f, "no keymap compiles from {names}: {reason}")
            }
            Self::Compose { locale, reason } if reason.is_empty() => {
                write!(f, "there is no compose table for `{locale}`")
            }
            Self::Compose { locale, reason } => {
                write!(f, "there is no compose table for `{locale}`: {reason}")
            }
            Self::Name { field } => write!(f, "the {field} holds a zero byte"),
            Self::Refused { what } => write!(f, "{what} answered with nothing"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    //! What each case reads as.

    use super::*;

    #[test]
    fn no_case_of_this_error_is_large_enough_to_weigh_down_a_result() {
        // Every call in this crate answers `Result<_, Error>`, so the largest case here is the
        // floor on all of them. `RuleNames` is five optional strings and would triple it.
        assert!(
            size_of::<Error>() <= 64,
            "an error is {} bytes",
            size_of::<Error>()
        );
    }

    #[test]
    fn a_keymap_that_said_why_says_why() {
        let names = RuleNames {
            layout: Some("zz".to_owned()),
            ..RuleNames::default()
        };
        let quiet = Error::Keymap {
            names: Box::new(names.clone()),
            reason: String::new(),
        };
        let loud = Error::Keymap {
            names: Box::new(names),
            reason: "Couldn't look up rules".to_owned(),
        };

        assert_eq!(quiet.to_string(), "no keymap compiles from layout=zz");
        assert_eq!(
            loud.to_string(),
            "no keymap compiles from layout=zz: Couldn't look up rules"
        );
    }
}
