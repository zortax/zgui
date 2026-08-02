//! A clipboard that is an ordinary value in memory.

use std::sync::Mutex;

use zgui_platform::{
    Clipboard, ClipboardData, ClipboardError, ClipboardFormat, ClipboardKind, ClipboardSerial,
    ClipboardSerials, ClipboardWriteOptions,
};

/// A clipboard that is an ordinary value in memory.
///
/// The standard clipboard and the selection are separate slots. A backend that shared one would
/// pass every test written against copy and paste while destroying the user's selection on every
/// copy, which is a fault nothing but a person notices.
///
/// ```
/// use zgui_platform::{Clipboard, ClipboardData, ClipboardKind, ClipboardWriteOptions};
/// use zgui_platform_headless::MemoryClipboard;
///
/// let clipboard = MemoryClipboard::default();
/// clipboard
///     .write(
///         ClipboardKind::Standard,
///         ClipboardData::from("copied"),
///         ClipboardWriteOptions::default(),
///     )
///     .expect("an in-memory clipboard always accepts text");
///
/// assert!(clipboard.read_blocking(ClipboardKind::Primary, zgui_platform::ClipboardFormat::Text).is_err());
/// ```
#[derive(Debug, Default)]
pub struct MemoryClipboard {
    /// Identifiers handed to reads that are answered through the loop.
    serials: ClipboardSerials,
    /// What was last copied.
    standard: Mutex<Option<ClipboardData>>,
    /// What was last selected.
    primary: Mutex<Option<ClipboardData>>,
}

impl MemoryClipboard {
    /// Which slot a clipboard kind is held in.
    fn slot(&self, kind: ClipboardKind) -> &Mutex<Option<ClipboardData>> {
        match kind {
            ClipboardKind::Primary => &self.primary,
            _ => &self.standard,
        }
    }
}

impl Clipboard for MemoryClipboard {
    fn read(&self, _kind: ClipboardKind, _format: ClipboardFormat) -> ClipboardSerial {
        self.serials.take()
    }

    fn read_blocking(
        &self,
        kind: ClipboardKind,
        format: ClipboardFormat,
    ) -> Result<ClipboardData, ClipboardError> {
        let held = self
            .slot(kind)
            .lock()
            .expect("the clipboard is not poisoned")
            .clone()
            .ok_or(ClipboardError::Empty(kind))?;
        if held.format() == format {
            Ok(held)
        } else {
            Err(ClipboardError::UnavailableFormat(format))
        }
    }

    fn write(
        &self,
        kind: ClipboardKind,
        data: ClipboardData,
        _options: ClipboardWriteOptions,
    ) -> Result<(), ClipboardError> {
        *self
            .slot(kind)
            .lock()
            .expect("the clipboard is not poisoned") = Some(data);
        Ok(())
    }

    fn clear(&self, kind: ClipboardKind) -> Result<(), ClipboardError> {
        *self
            .slot(kind)
            .lock()
            .expect("the clipboard is not poisoned") = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryClipboard;
    use zgui_platform::{
        Clipboard, ClipboardData, ClipboardFormat, ClipboardKind, ClipboardWriteOptions,
    };

    #[test]
    fn copying_does_not_overwrite_the_selection() {
        let clipboard = MemoryClipboard::default();
        clipboard
            .write(
                ClipboardKind::Primary,
                ClipboardData::from("selected"),
                ClipboardWriteOptions::default(),
            )
            .expect("accepted");
        clipboard
            .write(
                ClipboardKind::Standard,
                ClipboardData::from("copied"),
                ClipboardWriteOptions::default(),
            )
            .expect("accepted");

        let selected = clipboard
            .read_blocking(ClipboardKind::Primary, ClipboardFormat::Text)
            .expect("still there");
        assert_eq!(selected.as_text(), Some("selected"));
    }

    #[test]
    fn every_read_started_through_the_loop_gets_its_own_identity() {
        let clipboard = MemoryClipboard::default();
        let first = clipboard.read(ClipboardKind::Standard, ClipboardFormat::Text);
        let second = clipboard.read(ClipboardKind::Standard, ClipboardFormat::Text);
        assert_ne!(first, second);
    }
}
