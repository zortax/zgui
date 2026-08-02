//! What can go wrong at the platform boundary.

use thiserror::Error;

/// A request the running platform cannot satisfy.
///
/// This is not a failure. Every desktop refuses something — a window cannot place itself under a
/// compositor that forbids it, a pointer cannot be confined where no protocol exists for it — and
/// a caller's correct response is almost always to do without rather than to stop. Returning it
/// as an error rather than ignoring the call is what lets a caller *know* it did not happen, which
/// is the difference between an interface that degrades and one that lies.
///
/// Ask [`PlatformCapabilities`](crate::PlatformCapabilities) before making a request whose refusal
/// would change what should be drawn; use this for the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Error)]
#[error("the platform does not support this request")]
pub struct Unsupported;

/// A platform operation that failed.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum PlatformError {
    /// The platform cannot do this at all.
    #[error(transparent)]
    Unsupported(#[from] Unsupported),
    /// A surface could not be created.
    #[error("the surface could not be created: {0}")]
    SurfaceCreation(String),
    /// The operation names a surface that no longer exists.
    #[error("that surface no longer exists")]
    NoSuchSurface,
    /// The platform refused for a reason of its own, described in its own words.
    #[error("{0}")]
    Backend(String),
}

#[cfg(test)]
mod tests {
    use super::{PlatformError, Unsupported};

    #[test]
    fn an_unsupported_request_converts_into_the_general_error() {
        let error: PlatformError = Unsupported.into();
        assert_eq!(error, PlatformError::Unsupported(Unsupported));
    }

    #[test]
    fn every_error_says_what_happened() {
        for error in [
            PlatformError::from(Unsupported),
            PlatformError::SurfaceCreation("no display".to_owned()),
            PlatformError::NoSuchSurface,
            PlatformError::Backend("compositor refused".to_owned()),
        ] {
            assert!(!error.to_string().is_empty());
        }
    }
}
