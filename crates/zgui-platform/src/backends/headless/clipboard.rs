//! A clipboard that is an ordinary value in memory.

use std::sync::Mutex;

use crate::clipboard::{
    Clipboard, ClipboardData, ClipboardError, ClipboardFormat, ClipboardKind, ClipboardSerial,
    ClipboardSerials, ClipboardWriteOptions,
};

/// A clipboard that is an ordinary value in memory.
///
/// The two clipboards are separate slots, because a backend that shared one would pass every test
/// written against copy and paste while destroying the user's selection on every copy.
#[derive(Default)]
pub(super) struct MemoryClipboard {
    serials: ClipboardSerials,
    standard: Mutex<Option<ClipboardData>>,
    primary: Mutex<Option<ClipboardData>>,
}

impl MemoryClipboard {
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
