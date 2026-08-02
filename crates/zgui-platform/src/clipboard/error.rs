//! Why a clipboard operation did not happen.

use thiserror::Error;

use crate::clipboard::data::{ClipboardFormat, ClipboardKind};

/// Why a clipboard read or write did not happen.
///
/// The three ordinary answers are told apart because a caller does different things with each: an
/// empty clipboard disables a paste command, an unavailable representation falls back to another
/// one, and an unavailable clipboard hides the command entirely.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ClipboardError {
    /// There is nothing on this clipboard at all.
    #[error("the {0:?} clipboard is empty")]
    Empty(ClipboardKind),
    /// There is something, but not in the representation that was asked for.
    #[error("the clipboard holds nothing in the {0:?} representation")]
    UnavailableFormat(ClipboardFormat),
    /// This platform has no such clipboard.
    ///
    /// The selection clipboard is the case that matters: it exists on most desktops and on none
    /// of the others, so a control that offers a middle-click paste has to be able to find out.
    #[error("this platform has no {0:?} clipboard")]
    UnsupportedKind(ClipboardKind),
    /// This platform cannot produce that representation at all, whatever is on the clipboard.
    #[error("this platform cannot read or write the {0:?} representation")]
    UnsupportedFormat(ClipboardFormat),
    /// The owner of the selection did not answer in time.
    #[error("the clipboard owner did not answer")]
    TimedOut,
    /// The platform refused, in its own words.
    #[error("{0}")]
    Backend(String),
}

impl ClipboardError {
    /// Whether this means the platform can never do it, as opposed to could not do it now.
    ///
    /// A permanent refusal is worth remembering — it is the difference between hiding a command
    /// and greying it out — and a temporary one is not.
    pub const fn is_permanent(&self) -> bool {
        matches!(self, Self::UnsupportedKind(_) | Self::UnsupportedFormat(_))
    }
}

#[cfg(test)]
mod tests {
    use super::ClipboardError;
    use crate::clipboard::data::{ClipboardFormat, ClipboardKind};

    #[test]
    fn only_the_unsupported_answers_are_permanent() {
        assert!(ClipboardError::UnsupportedKind(ClipboardKind::Primary).is_permanent());
        assert!(ClipboardError::UnsupportedFormat(ClipboardFormat::Image).is_permanent());
        assert!(!ClipboardError::Empty(ClipboardKind::Standard).is_permanent());
        assert!(!ClipboardError::TimedOut.is_permanent());
    }

    #[test]
    fn every_answer_says_which_clipboard_or_representation_it_is_about() {
        assert!(
            ClipboardError::Empty(ClipboardKind::Primary)
                .to_string()
                .contains("Primary")
        );
        assert!(
            ClipboardError::UnsupportedFormat(ClipboardFormat::Html)
                .to_string()
                .contains("Html")
        );
    }
}
