//! What can go wrong when a face is registered.

use core::fmt::{self, Display};

/// Why a font file could not be registered.
///
/// Registration is the only fallible operation on a font source, and it is fallible for one reason:
/// the bytes came from outside — a downloaded `@font-face`, a file the application shipped — and
/// may not be a font at all. Resolution is not fallible in the same way; a family with no face
/// simply has none, which is an ordinary outcome and not an error.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FontError {
    /// The bytes are not in a font format the source understands.
    Unrecognised,
    /// The bytes are a recognised format but are damaged past the point of reading a face from.
    Malformed(&'static str),
    /// The file is a valid font with no faces in it, which a malformed collection index produces.
    Empty,
}

impl Display for FontError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unrecognised => formatter.write_str("not a recognised font format"),
            Self::Malformed(detail) => write!(formatter, "malformed font: {detail}"),
            Self::Empty => formatter.write_str("the font file holds no faces"),
        }
    }
}

impl core::error::Error for FontError {}
