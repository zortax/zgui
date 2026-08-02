//! What is on a clipboard, and which of the several clipboards it is on.

use std::path::PathBuf;

use zgui_vocab::SharedString;

/// Which of the platform's clipboards is meant.
///
/// Most desktops have two and they behave completely differently. The standard one is what an
/// explicit copy writes and an explicit paste reads. The selection one, where it exists, is
/// written merely by selecting text and read by a middle click, with no explicit action either
/// way — so writing to it by mistake destroys whatever the user had selected elsewhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ClipboardKind {
    /// The clipboard an explicit copy and paste use.
    #[default]
    Standard,
    /// The clipboard that selecting text writes and a middle click reads.
    Primary,
}

/// Which representation of the clipboard's contents is wanted.
///
/// Asking for a representation the platform cannot produce is answered with an error rather than
/// with an empty value, because the two mean opposite things to a caller deciding whether to
/// enable a paste command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ClipboardFormat {
    /// Plain text.
    #[default]
    Text,
    /// Rich text as markup.
    Html,
    /// A list of file paths.
    FileList,
    /// An image.
    Image,
}

/// What is on the clipboard.
///
/// The variants line up one to one with [`ClipboardFormat`], and a read answers with the variant
/// its request named.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ClipboardData {
    /// Plain text.
    Text(SharedString),
    /// Rich text as markup, with the plain-text form to fall back on.
    ///
    /// Both are carried because a paste target that cannot render markup still has something to
    /// insert, and deriving one from the other by stripping tags produces the wrong answer often
    /// enough to be worth avoiding.
    Html {
        /// The markup.
        markup: SharedString,
        /// The same content as plain text, when the writer supplied it.
        text: Option<SharedString>,
    },
    /// A list of file paths.
    FileList(Vec<PathBuf>),
    /// An image, as encoded bytes with the media type that says how to decode them.
    ///
    /// Encoded rather than decoded because this crate has no image decoder and must not acquire
    /// one: a clipboard contract that named a pixel buffer would drag a decoder into the platform
    /// layer for every backend.
    Image {
        /// What kind of image this is.
        media_type: SharedString,
        /// The encoded bytes.
        bytes: Vec<u8>,
    },
}

impl ClipboardData {
    /// Which representation this is.
    pub const fn format(&self) -> ClipboardFormat {
        match self {
            Self::Text(_) => ClipboardFormat::Text,
            Self::Html { .. } => ClipboardFormat::Html,
            Self::FileList(_) => ClipboardFormat::FileList,
            Self::Image { .. } => ClipboardFormat::Image,
        }
    }

    /// The plain-text reading of this content, when there is one.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            Self::Html { text, .. } => text.as_deref(),
            _ => None,
        }
    }
}

impl From<&str> for ClipboardData {
    fn from(text: &str) -> Self {
        Self::Text(SharedString::from(text))
    }
}

/// How a write should be treated by the desktop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ClipboardWriteOptions {
    /// Asks clipboard managers not to record this value.
    ///
    /// This is what stops a password a user copied from appearing in a clipboard history for the
    /// rest of the session. It is a request: a desktop that does not honour it will still record
    /// the value, so it is a mitigation and not a guarantee, and a password field should not
    /// pretend otherwise.
    pub exclude_from_history: bool,
}

impl ClipboardWriteOptions {
    /// The options for a value that must not be recorded.
    pub const SECRET: Self = Self {
        exclude_from_history: true,
    };
}

#[cfg(test)]
mod tests {
    use super::{ClipboardData, ClipboardFormat, ClipboardKind, ClipboardWriteOptions};

    #[test]
    fn content_reports_the_format_it_answers_for() {
        assert_eq!(ClipboardData::from("hi").format(), ClipboardFormat::Text);
        assert_eq!(
            ClipboardData::FileList(Vec::new()).format(),
            ClipboardFormat::FileList
        );
    }

    #[test]
    fn markup_carries_its_own_plain_text_rather_than_deriving_one() {
        let with_text = ClipboardData::Html {
            markup: "<b>hi</b>".into(),
            text: Some("hi".into()),
        };
        assert_eq!(with_text.as_text(), Some("hi"));

        let without = ClipboardData::Html {
            markup: "<b>hi</b>".into(),
            text: None,
        };
        assert_eq!(without.as_text(), None);
    }

    #[test]
    fn the_standard_clipboard_is_the_one_meant_by_default() {
        assert_eq!(ClipboardKind::default(), ClipboardKind::Standard);
        assert_eq!(ClipboardFormat::default(), ClipboardFormat::Text);
        assert!(!ClipboardWriteOptions::default().exclude_from_history);
        let secret = ClipboardWriteOptions::SECRET;
        assert!(secret.exclude_from_history);
        assert_ne!(secret, ClipboardWriteOptions::default());
    }
}
