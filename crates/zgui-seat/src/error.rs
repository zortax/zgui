//! Refusals, and what each one carries.

use std::fmt;
use std::time::Duration;

/// The result of a call into libseat.
pub type Result<T> = std::result::Result<T, Error>;

/// What a call into libseat refused with.
///
/// Each case names what refused, and carries the `errno` the system left where there is one.
///
/// ```
/// use zgui_seat::Error;
///
/// let refused = Error::Seat {
///     call: "libseat_open_seat",
///     errno: 16,
/// };
/// let message = refused.to_string();
///
/// assert!(message.contains("libseat_open_seat"));
/// assert!(message.contains(&std::io::Error::from_raw_os_error(16).to_string()));
/// ```
// A case is added together with the code that builds it. `cargo xtask ledger inert` fails a variant
// that is matched on and built nowhere, because such a case reads like a working feature that is
// merely unused.
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// libseat could not be opened.
    ///
    /// The crate is shaped around this case. The library is opened at run time, so a build needs
    /// neither it nor a session daemon, and a machine without it hands the caller this value to
    /// read. A console session answers it by opening the devices itself, which needs root and a
    /// terminal reserved in advance.
    ///
    /// A machine that has libseat lands here too when something libseat needs is missing, such as
    /// libsystemd. `reason` tells the two apart, so a report carries it.
    Library {
        /// The sonames that were tried, in order.
        tried: Vec<String>,
        /// What the loader said about the last one.
        reason: String,
    },
    /// The library is here, and it does not carry a symbol this crate calls.
    ///
    /// A libseat older than the interface this was written against lands here, and so does a shared
    /// object that is not libseat at all. It is a separate case from an absent library because the
    /// answer differs: a caller reports which symbol is missing, and a person reads a version out
    /// of that.
    Symbol {
        /// The symbol that was asked for.
        name: &'static str,
        /// What the loader said, or why the address it gave cannot be used.
        reason: String,
    },
    /// libseat refused the seat.
    ///
    /// Every backend it was built with was tried and none opened one, or the one named by
    /// `LIBSEAT_BACKEND` refused. A session that already has a controlling client is the ordinary
    /// reason, and `errno` tells the reasons apart.
    Seat {
        /// The libseat call that refused.
        call: &'static str,
        /// `errno` at the refusal.
        errno: i32,
    },
    /// The seat opened and never became usable.
    ///
    /// A backend can accept the call and hand back a seat it cannot enable: the builtin backend
    /// with no terminal to take does exactly that. A seat that has not enabled inside the bound is
    /// a seat this session did not get, and waiting longer answers nothing.
    NeverEnabled {
        /// How long the seat was waited for.
        within: Duration,
    },
    /// libseat could not read its connection.
    ///
    /// Nothing more can be asked of the seat from here. A connection that failed carries no later
    /// change, so the devices this session holds are unusable, and the seat is closed.
    Dispatch {
        /// `errno` at the failure.
        errno: i32,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library { tried, reason } => write!(
                f,
                "libseat could not be opened; tried {}: {reason}",
                tried.join(", ")
            ),
            Self::Symbol { name, reason } => write!(f, "libseat has no `{name}`: {reason}"),
            Self::Seat { call, errno } => {
                write!(f, "libseat refused the seat; `{call}`: {}", os(*errno))
            }
            Self::NeverEnabled { within } => write!(
                f,
                "the seat opened and did not enable within {within:?}, so it is a seat this \
                 session did not get"
            ),
            Self::Dispatch { errno } => {
                write!(f, "the seat could not be dispatched: {}", os(*errno))
            }
        }
    }
}

/// Returns the system's description of one of its error numbers.
fn os(errno: i32) -> std::io::Error {
    std::io::Error::from_raw_os_error(errno)
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    //! What each case reads as.

    use super::*;

    #[test]
    fn an_absent_library_names_every_soname_and_says_why() {
        // A person reading this has to be able to check the machine against it, so the message
        // carries the whole list rather than the last name tried. It says nothing about why the
        // open failed beyond what the loader said: a present libseat with an absent libsystemd
        // arrives here too.
        let error = Error::Library {
            tried: vec!["libseat.so.1".to_owned(), "libseat.so".to_owned()],
            reason: "cannot open shared object file".to_owned(),
        };

        assert_eq!(
            error.to_string(),
            "libseat could not be opened; tried libseat.so.1, libseat.so: cannot open shared \
             object file"
        );
    }

    #[test]
    fn a_refused_seat_names_the_call_and_the_number_the_system_gave() {
        // The number tells a busy session from a missing daemon, and the call says where in the
        // open it happened, so a person can read both out of the line.
        let error = Error::Seat {
            call: "libseat_open_seat",
            errno: 16,
        };

        let message = error.to_string();
        assert!(
            message.starts_with("libseat refused the seat; `libseat_open_seat`: "),
            "the call is named: {message}"
        );
        assert!(message.contains("16"), "and the number is there: {message}");
    }

    #[test]
    fn a_seat_that_never_enabled_says_how_long_it_was_waited_for() {
        let error = Error::NeverEnabled {
            within: Duration::from_secs(2),
        };

        assert_eq!(
            error.to_string(),
            "the seat opened and did not enable within 2s, so it is a seat this session did not get"
        );
    }

    #[test]
    fn a_dispatch_failure_says_what_the_system_gave() {
        let error = Error::Dispatch { errno: 107 };

        let message = error.to_string();
        assert!(
            message.starts_with("the seat could not be dispatched: "),
            "the failure is named: {message}"
        );
        assert!(
            message.contains("107"),
            "and the number is there: {message}"
        );
    }

    #[test]
    fn a_missing_symbol_is_named() {
        let error = Error::Symbol {
            name: "libseat_switch_session",
            reason: "undefined symbol".to_owned(),
        };

        assert!(error.to_string().contains("libseat_switch_session"));
    }
}
