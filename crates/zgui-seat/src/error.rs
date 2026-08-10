//! Refusals, and what each one carries.

use std::ffi::c_int;
use std::fmt;
use std::path::PathBuf;
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
    /// libseat refused a device.
    ///
    /// The backend opens the device, so a path the machine does not have arrives here as well as a
    /// device the seat may not hand over. The seat has to be enabled, and the backends permit DRM
    /// and evdev devices alone.
    OpenDevice {
        /// The device that was asked for.
        path: PathBuf,
        /// `errno` at the refusal.
        errno: i32,
    },
    /// A device path holds a zero byte.
    ///
    /// A C string ends at its first zero, so such a path would arrive cut short and open a device
    /// nobody asked for. It is refused here, and libseat is asked nothing.
    DevicePath {
        /// The path that was asked for.
        path: PathBuf,
    },
    /// libseat refused to take a device back.
    ///
    /// The descriptor is closed either way. What is left is the session daemon's record of the
    /// device, which it holds until the seat closes.
    CloseDevice {
        /// libseat's id for the device.
        device: c_int,
        /// `errno` at the refusal.
        errno: i32,
    },
    /// libseat refused the switch.
    ///
    /// The session carries on unchanged. A backend with no terminals to switch between refuses
    /// every switch, and the noop backend is one.
    Switch {
        /// The terminal that was asked for.
        terminal: u32,
        /// `errno` at the refusal.
        errno: i32,
    },
    /// The terminal number is wider than libseat's interface holds.
    ///
    /// A session number crosses as a C `int`. A number that does not fit would arrive as a
    /// different one, so it is refused here and libseat is asked nothing.
    Terminal {
        /// The terminal that was asked for.
        terminal: u32,
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
            Self::OpenDevice { path, errno } => write!(
                f,
                "libseat refused the device `{}`: {}",
                path.display(),
                os(*errno)
            ),
            Self::DevicePath { path } => {
                write!(f, "the device path `{}` holds a zero byte", path.display())
            }
            Self::CloseDevice { device, errno } => write!(
                f,
                "libseat refused to take device {device} back: {}",
                os(*errno)
            ),
            Self::Switch { terminal, errno } => write!(
                f,
                "libseat refused the switch to terminal {terminal}: {}",
                os(*errno)
            ),
            Self::Terminal { terminal } => write!(
                f,
                "terminal {terminal} is wider than libseat's session number holds"
            ),
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
    fn a_refused_device_names_the_path_and_the_number_the_system_gave() {
        // Which device is the first thing a person asks, so the path is in the line rather than in
        // a field somebody has to go and read.
        let error = Error::OpenDevice {
            path: PathBuf::from("/dev/dri/card0"),
            errno: 13,
        };

        let message = error.to_string();
        assert!(
            message.starts_with("libseat refused the device `/dev/dri/card0`: "),
            "the device is named: {message}"
        );
        assert!(message.contains("13"), "and the number is there: {message}");
    }

    #[test]
    fn a_path_with_a_zero_byte_says_what_is_wrong_with_it() {
        let error = Error::DevicePath {
            path: PathBuf::from("/dev/null"),
        };

        assert_eq!(
            error.to_string(),
            "the device path `/dev/null` holds a zero byte"
        );
    }

    #[test]
    fn a_device_that_did_not_go_back_names_its_id() {
        let error = Error::CloseDevice {
            device: 7,
            errno: 22,
        };

        let message = error.to_string();
        assert!(
            message.starts_with("libseat refused to take device 7 back: "),
            "the device is named: {message}"
        );
        assert!(message.contains("22"), "and the number is there: {message}");
    }

    #[test]
    fn a_refused_switch_names_the_terminal() {
        let error = Error::Switch {
            terminal: 1,
            errno: 19,
        };

        let message = error.to_string();
        assert!(
            message.starts_with("libseat refused the switch to terminal 1: "),
            "the terminal is named: {message}"
        );
        assert!(message.contains("19"), "and the number is there: {message}");
    }

    #[test]
    fn a_terminal_the_interface_cannot_hold_says_so() {
        let error = Error::Terminal { terminal: u32::MAX };

        assert_eq!(
            error.to_string(),
            "terminal 4294967295 is wider than libseat's session number holds"
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
