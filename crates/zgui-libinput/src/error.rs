//! The errors a call into libinput answers with.

use std::fmt;

/// The result of a call into libinput.
pub type Result<T> = std::result::Result<T, Error>;

/// The reason a call into libinput failed.
///
/// The enumeration is `non_exhaustive`, so a match on it carries a wildcard arm.
///
/// ```
/// use zgui_libinput::Error;
///
/// let error = Error::Symbol {
///     name: "libinput_dispatch",
///     reason: "undefined symbol".to_owned(),
/// };
///
/// assert_eq!(
///     error.to_string(),
///     "libinput has no `libinput_dispatch`: undefined symbol"
/// );
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// libinput could not be opened.
    ///
    /// The library is opened at run time, so a build needs neither it nor a device. A machine
    /// without libinput answers with this value. A console session answers it by reading the
    /// devices itself, and gives up the acceleration, the touchpad handling and the quirks
    /// database that come with libinput.
    ///
    /// A machine that has libinput answers with this value as well when something libinput needs
    /// is missing, such as libevdev or libwacom. `reason` tells the two apart, so a report carries
    /// it.
    Library {
        /// The sonames that were tried, in order.
        tried: Vec<String>,
        /// What the loader said about the last one.
        reason: String,
    },
    /// The library is here, and it does not carry a symbol this crate calls.
    ///
    /// A libinput older than the interface this was written against answers with this value, and
    /// so does a shared object that is not libinput. `name` is the symbol a person reads a version
    /// out of.
    Symbol {
        /// The symbol that was asked for.
        name: &'static str,
        /// What the loader said, or why the address it gave cannot be used.
        reason: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library { tried, reason } => write!(
                f,
                "libinput could not be opened; tried {}: {reason}",
                tried.join(", ")
            ),
            Self::Symbol { name, reason } => write!(f, "libinput has no `{name}`: {reason}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    //! What each case reads as.

    use super::*;

    #[test]
    fn an_absent_library_names_every_soname_and_says_why() {
        // A person reading this has to be able to check the machine against it, so the message
        // carries the whole list rather than the last name tried, and says nothing about *why* the
        // open failed beyond what the loader said: a present libinput with an absent libwacom
        // arrives here too.
        let error = Error::Library {
            tried: vec!["libinput.so.10".to_owned(), "libinput.so".to_owned()],
            reason: "cannot open shared object file".to_owned(),
        };

        assert_eq!(
            error.to_string(),
            "libinput could not be opened; tried libinput.so.10, libinput.so: cannot open shared \
             object file"
        );
    }

    #[test]
    fn a_missing_symbol_is_named() {
        // The symbol name is what a person reads a version out of, so the message carries it
        // rather than a field somebody has to go and read.
        let error = Error::Symbol {
            name: "libinput_event_pointer_get_scroll_value_v120",
            reason: "undefined symbol".to_owned(),
        };

        assert_eq!(
            error.to_string(),
            "libinput has no `libinput_event_pointer_get_scroll_value_v120`: undefined symbol"
        );
    }
}
