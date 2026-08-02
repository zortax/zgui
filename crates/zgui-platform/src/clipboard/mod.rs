//! Reading and writing the desktop's clipboards.

mod data;
mod error;
mod serial;

pub use crate::clipboard::data::{
    ClipboardData, ClipboardFormat, ClipboardKind, ClipboardWriteOptions,
};
pub use crate::clipboard::error::ClipboardError;
pub use crate::clipboard::serial::{ClipboardSerial, ClipboardSerials};

/// Reading and writing the desktop's clipboards.
///
/// # Why a read is a request rather than a return value
///
/// A clipboard read is not a memory read. The content belongs to another process, which has to be
/// asked for it and may take as long as it likes to answer — or never answer at all. Some
/// platforms have no synchronous form of the question in the first place.
///
/// So [`Clipboard::read`] starts a read and returns an identifier, and the answer arrives later as
/// a wake carrying that identifier. Every backend can implement that shape: one that *can* answer
/// immediately answers immediately, by delivering the wake before it returns to the loop. The
/// reverse is not true, which is why this is the shape.
///
/// [`Clipboard::read_blocking`] exists because on a desktop the immediate answer is genuinely
/// available and threading a request through the loop for it is ceremony. It is allowed to refuse.
///
/// # Why the write is not symmetrical
///
/// A write completes or fails then and there: the value is handed over, and on the platforms where
/// ownership of a selection is retained rather than copied, the backend keeps serving it. Nothing
/// about that needs a request identifier.
///
/// # What a caller may assume about representations
///
/// Only that plain text works. Everything else is genuinely absent on some real desktops, and the
/// answer is [`ClipboardError::UnsupportedFormat`] rather than an empty value, so a caller can
/// fall back rather than silently pasting nothing. Ask
/// [`PlatformCapabilities`](crate::PlatformCapabilities) up front to decide what to *offer*, and
/// handle the error to decide what to *do*.
pub trait Clipboard {
    /// Starts a read; the answer arrives as a wake carrying the returned identifier.
    fn read(&self, kind: ClipboardKind, format: ClipboardFormat) -> ClipboardSerial;

    /// Reads without going through the loop, where the platform can answer at once.
    ///
    /// A backend with no synchronous form refuses, and the caller falls back to
    /// [`Clipboard::read`].
    fn read_blocking(
        &self,
        kind: ClipboardKind,
        format: ClipboardFormat,
    ) -> Result<ClipboardData, ClipboardError>;

    /// Puts `data` on the clipboard.
    fn write(
        &self,
        kind: ClipboardKind,
        data: ClipboardData,
        options: ClipboardWriteOptions,
    ) -> Result<(), ClipboardError>;

    /// Empties the clipboard.
    fn clear(&self, kind: ClipboardKind) -> Result<(), ClipboardError>;
}

#[cfg(test)]
mod tests {
    use super::{
        Clipboard, ClipboardData, ClipboardError, ClipboardFormat, ClipboardKind, ClipboardSerial,
        ClipboardSerials, ClipboardWriteOptions,
    };
    use std::sync::Mutex;

    /// A clipboard that holds one plain-text value and nothing else, as a real desktop backend
    /// with only a text protocol available does.
    #[derive(Default)]
    struct TextOnly {
        serials: ClipboardSerials,
        held: Mutex<Option<ClipboardData>>,
    }

    impl Clipboard for TextOnly {
        fn read(&self, kind: ClipboardKind, format: ClipboardFormat) -> ClipboardSerial {
            let _ = (kind, format);
            self.serials.take()
        }

        fn read_blocking(
            &self,
            kind: ClipboardKind,
            format: ClipboardFormat,
        ) -> Result<ClipboardData, ClipboardError> {
            if format != ClipboardFormat::Text {
                return Err(ClipboardError::UnsupportedFormat(format));
            }
            self.held
                .lock()
                .expect("the clipboard is not poisoned")
                .clone()
                .ok_or(ClipboardError::Empty(kind))
        }

        fn write(
            &self,
            kind: ClipboardKind,
            data: ClipboardData,
            options: ClipboardWriteOptions,
        ) -> Result<(), ClipboardError> {
            let _ = (kind, options);
            if data.format() != ClipboardFormat::Text {
                return Err(ClipboardError::UnsupportedFormat(data.format()));
            }
            *self.held.lock().expect("the clipboard is not poisoned") = Some(data);
            Ok(())
        }

        fn clear(&self, kind: ClipboardKind) -> Result<(), ClipboardError> {
            let _ = kind;
            *self.held.lock().expect("the clipboard is not poisoned") = None;
            Ok(())
        }
    }

    #[test]
    fn a_text_only_backend_satisfies_the_whole_contract() {
        let clipboard: &dyn Clipboard = &TextOnly::default();
        assert_eq!(
            clipboard.read_blocking(ClipboardKind::Standard, ClipboardFormat::Text),
            Err(ClipboardError::Empty(ClipboardKind::Standard))
        );
        clipboard
            .write(
                ClipboardKind::Standard,
                ClipboardData::from("hello"),
                ClipboardWriteOptions::default(),
            )
            .expect("text is writable");
        assert_eq!(
            clipboard
                .read_blocking(ClipboardKind::Standard, ClipboardFormat::Text)
                .expect("text is readable")
                .as_text(),
            Some("hello")
        );
    }

    #[test]
    fn a_representation_the_backend_lacks_is_an_error_rather_than_an_empty_value() {
        let clipboard = TextOnly::default();
        let refusal = clipboard.read_blocking(ClipboardKind::Standard, ClipboardFormat::Image);
        assert_eq!(
            refusal,
            Err(ClipboardError::UnsupportedFormat(ClipboardFormat::Image))
        );
        assert!(refusal.unwrap_err().is_permanent());
    }

    #[test]
    fn every_read_gets_its_own_identifier() {
        let clipboard = TextOnly::default();
        let first = clipboard.read(ClipboardKind::Standard, ClipboardFormat::Text);
        let second = clipboard.read(ClipboardKind::Primary, ClipboardFormat::Text);
        assert_ne!(first, second);
    }

    #[test]
    fn clearing_empties_the_clipboard() {
        let clipboard = TextOnly::default();
        clipboard
            .write(
                ClipboardKind::Standard,
                ClipboardData::from("x"),
                ClipboardWriteOptions::SECRET,
            )
            .expect("text is writable");
        clipboard.clear(ClipboardKind::Standard).expect("clearable");
        assert!(
            clipboard
                .read_blocking(ClipboardKind::Standard, ClipboardFormat::Text)
                .is_err()
        );
    }
}
