//! A clipboard that is an ordinary value in memory.

use std::sync::{Arc, Mutex};

use crate::app::WakeReason;
use crate::clipboard::{
    Clipboard, ClipboardData, ClipboardError, ClipboardFormat, ClipboardKind, ClipboardSerial,
    ClipboardSerials, ClipboardWriteOptions,
};
use crate::waker::Waker;

/// A clipboard that is an ordinary value in memory.
///
/// The two clipboards are separate slots, because a backend that shared one would pass every test
/// written against copy and paste while destroying the user's selection on every copy.
#[derive(Default)]
pub(super) struct MemoryClipboard {
    serials: ClipboardSerials,
    standard: Mutex<Option<ClipboardData>>,
    primary: Mutex<Option<ClipboardData>>,
    /// How a started read delivers its answer.
    waker: Mutex<Option<Arc<dyn Waker>>>,
}

impl MemoryClipboard {
    /// Names how a read started through the loop delivers its answer.
    pub(super) fn attach_waker(&self, waker: Arc<dyn Waker>) {
        *self.waker.lock().expect("the clipboard is not poisoned") = Some(waker);
    }

    fn slot(&self, kind: ClipboardKind) -> &Mutex<Option<ClipboardData>> {
        match kind {
            ClipboardKind::Primary => &self.primary,
            _ => &self.standard,
        }
    }
}

impl Clipboard for MemoryClipboard {
    fn read(&self, kind: ClipboardKind, format: ClipboardFormat) -> ClipboardSerial {
        let serial = self.serials.take();
        let result = self.read_blocking(kind, format);
        let waker = self
            .waker
            .lock()
            .expect("the clipboard is not poisoned")
            .clone();
        // With no waker the answer is dropped, which is what a real backend does when a read is
        // started before the loop can hear the reply.
        if let Some(waker) = waker {
            waker.wake(WakeReason::ClipboardRead { serial, result });
        }
        serial
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
