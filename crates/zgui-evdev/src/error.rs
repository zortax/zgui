//! Refusals, and what each one carries.

use std::fmt;
use std::path::PathBuf;

/// The result of a call into the kernel.
pub type Result<T> = std::result::Result<T, Error>;

/// What a call into the kernel refused with.
///
/// Each case names what refused and carries what a caller acts on: the path that was tried, the
/// request that was made, and the system's own reason where there is one.
///
/// ```
/// use std::path::PathBuf;
/// use zgui_evdev::Error;
///
/// let refused = Error::Open {
///     path: PathBuf::from("/dev/input/event0"),
///     source: std::io::Error::from_raw_os_error(13),
/// };
/// let message = refused.to_string();
///
/// assert!(message.contains("/dev/input/event0"));
/// assert!(message.contains(&std::io::Error::from_raw_os_error(13).to_string()));
/// ```
// A case is added together with the code that builds it. `cargo xtask ledger inert` fails a variant
// that is matched on and built nowhere, because such a case reads like a working feature that is
// merely unused.
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// The device could not be opened.
    ///
    /// Permission is the ordinary case. A `/dev/input/event*` node belongs to the `input` group on
    /// most systems, so a process outside that group reads none of them.
    Open {
        /// The path that was tried.
        path: PathBuf,
        /// Why it failed.
        source: std::io::Error,
    },
    /// An ioctl failed.
    Ioctl {
        /// What was being asked for, for a message a reader can act on.
        request: &'static str,
        /// Why it failed.
        source: std::io::Error,
    },
    /// The request could not be built, so nothing was asked.
    ///
    /// The crate refuses here: an event type past `EV_MAX` or an axis past `ABS_MAX`, whose request
    /// number would run into another request's range, and a length past the fourteen bits a request
    /// number has for it.
    Unusable(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { path, source } => write!(f, "cannot open {}: {source}", path.display()),
            Self::Ioctl { request, source } => write!(f, "{request} failed: {source}"),
            Self::Unusable(what) => write!(f, "{what}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } | Self::Ioctl { source, .. } => Some(source),
            Self::Unusable(_) => None,
        }
    }
}
