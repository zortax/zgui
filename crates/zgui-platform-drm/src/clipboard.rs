//! A clipboard on a machine that has none.

use std::sync::Arc;

use zgui_platform::{
    Clipboard, ClipboardData, ClipboardError, ClipboardFormat, ClipboardKind, ClipboardSerial,
    ClipboardSerials, ClipboardWriteOptions, WakeReason, Waker,
};

/// The clipboard of a bare console, which holds nothing.
///
/// A clipboard is a protocol between programs: one process owns a selection and serves it to
/// whoever asks. A bare console runs no such protocol, so there is nothing to ask and nothing that
/// would keep a value after this program ends. Every read and every write is refused, and the
/// refusal is permanent. A caller reads that as a reason to hide its copy and paste commands.
///
/// A clipboard here would come from a session: a Wayland compositor, or a daemon that owns the
/// selection on the console's behalf. This backend runs neither. When one exists, this is where it
/// is reached and the refusals become answers.
///
/// ```
/// use std::sync::Arc;
/// use zgui_platform::{Clipboard, ClipboardFormat, ClipboardKind};
/// use zgui_platform_drm::{ConsoleClipboard, EventfdWaker};
///
/// let waker = Arc::new(EventfdWaker::new().expect("a wake channel is openable"));
/// let clipboard = ConsoleClipboard::new(waker);
///
/// let refusal = clipboard
///     .read_blocking(ClipboardKind::Standard, ClipboardFormat::Text)
///     .expect_err("a bare console has no clipboard");
///
/// assert!(refusal.is_permanent(), "so a caller can hide the command instead of retrying");
/// ```
pub struct ConsoleClipboard {
    /// Identifiers handed to reads that are answered through the loop.
    serials: ClipboardSerials,
    /// How the answer to a read reaches the loop.
    waker: Arc<dyn Waker>,
}

impl ConsoleClipboard {
    /// Creates a clipboard that answers through `waker`.
    pub fn new(waker: Arc<dyn Waker>) -> Self {
        Self {
            serials: ClipboardSerials::default(),
            waker,
        }
    }
}

impl core::fmt::Debug for ConsoleClipboard {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ConsoleClipboard")
    }
}

impl Clipboard for ConsoleClipboard {
    fn read(&self, kind: ClipboardKind, format: ClipboardFormat) -> ClipboardSerial {
        let serial = self.serials.take();
        // The answer is known at once, so it is delivered before this returns. A read that got an
        // identifier and never an answer would leave whatever asked waiting for ever.
        self.waker.wake(WakeReason::ClipboardRead {
            serial,
            result: self.read_blocking(kind, format),
        });
        serial
    }

    fn read_blocking(
        &self,
        kind: ClipboardKind,
        _format: ClipboardFormat,
    ) -> Result<ClipboardData, ClipboardError> {
        // Refused for the kind rather than reported empty: an empty clipboard is one that could
        // hold something, and this console has no clipboard at all.
        Err(ClipboardError::UnsupportedKind(kind))
    }

    fn write(
        &self,
        kind: ClipboardKind,
        _data: ClipboardData,
        _options: ClipboardWriteOptions,
    ) -> Result<(), ClipboardError> {
        Err(ClipboardError::UnsupportedKind(kind))
    }

    fn clear(&self, kind: ClipboardKind) -> Result<(), ClipboardError> {
        Err(ClipboardError::UnsupportedKind(kind))
    }
}

#[cfg(test)]
mod tests {
    use super::ConsoleClipboard;
    use crate::waker::EventfdWaker;
    use std::sync::Arc;
    use zgui_platform::{
        Clipboard, ClipboardData, ClipboardFormat, ClipboardKind, ClipboardWriteOptions,
        WakeReason, Waker,
    };

    /// A clipboard and the channel its answers arrive on.
    fn clipboard() -> (ConsoleClipboard, Arc<EventfdWaker>) {
        let waker = Arc::new(EventfdWaker::new().expect("a wake channel is openable"));
        let clipboard = ConsoleClipboard::new(Arc::clone(&waker) as Arc<dyn Waker>);
        (clipboard, waker)
    }

    #[test]
    fn a_console_refuses_permanently_so_a_command_can_be_hidden() {
        let (clipboard, _waker) = clipboard();
        let refusal = clipboard
            .read_blocking(ClipboardKind::Standard, ClipboardFormat::Text)
            .expect_err("a console has no clipboard");
        assert!(refusal.is_permanent());
        assert!(
            clipboard
                .write(
                    ClipboardKind::Standard,
                    ClipboardData::from("dropped"),
                    ClipboardWriteOptions::default(),
                )
                .is_err()
        );
        assert!(clipboard.clear(ClipboardKind::Standard).is_err());
    }

    #[test]
    fn a_read_through_the_loop_is_answered_rather_than_left_waiting() {
        let (clipboard, waker) = clipboard();
        let serial = clipboard.read(ClipboardKind::Primary, ClipboardFormat::Text);

        let delivered = waker.drain();
        assert_eq!(delivered.len(), 1);
        let WakeReason::ClipboardRead {
            serial: answered,
            result,
        } = &delivered[0]
        else {
            panic!("a read is answered as a clipboard read: {:?}", delivered[0]);
        };
        assert_eq!(*answered, serial);
        assert!(result.is_err(), "the answer is that there is no clipboard");
    }

    #[test]
    fn every_read_gets_its_own_identity() {
        let (clipboard, _waker) = clipboard();
        let first = clipboard.read(ClipboardKind::Standard, ClipboardFormat::Text);
        let second = clipboard.read(ClipboardKind::Standard, ClipboardFormat::Text);
        assert_ne!(first, second);
    }
}
