//! Refusals from the kernel, and what each one carries.

use std::fmt;

/// The result of a call into the kernel.
pub type Result<T> = std::result::Result<T, Error>;

/// What a call into the kernel refused with.
///
/// ```
/// use zgui_drm::Error;
///
/// let refused = Error::Ioctl {
///     request: "MODE_ATOMIC",
///     source: std::io::Error::from_raw_os_error(22),
/// };
/// let message = refused.to_string();
///
/// assert!(message.starts_with("MODE_ATOMIC failed: "));
/// assert!(message.contains(&std::io::Error::from_raw_os_error(22).to_string()));
/// ```
// A case is added together with the code that builds it. `cargo xtask ledger inert` fails a variant
// that is matched on and built nowhere, because such a case reads like a working feature that is
// merely unused.
#[non_exhaustive]
#[derive(Debug)]
pub enum Error {
    /// The device could not be opened.
    Open {
        /// The path that was tried.
        path: std::path::PathBuf,
        /// Why it failed.
        source: std::io::Error,
    },
    /// An ioctl failed.
    Ioctl {
        /// The name of the request, so that a reader can act on the message.
        request: &'static str,
        /// Why it failed.
        source: std::io::Error,
    },
    /// The kernel answered, and the answer cannot be used.
    ///
    /// A count that grew between the two passes of an enumeration, a blob of the wrong length, a
    /// property of a type this crate does not model.
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
