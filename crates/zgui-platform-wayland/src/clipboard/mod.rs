//! The desktop's clipboards, over the connection the window already owns.

pub mod mime;
pub mod pipe;
pub mod selection;

pub use crate::clipboard::selection::Selections;

use std::sync::Arc;

use zgui_platform::{
    Clipboard, ClipboardData, ClipboardError, ClipboardFormat, ClipboardKind, ClipboardSerial,
    ClipboardSerials, ClipboardWriteOptions, WakeReason,
};

/// The compositor's data device, seen as the two clipboards a desktop has.
///
/// # Why a read is a request and not a return value
///
/// The content belongs to another process. Reading it means asking that process to write into a
/// pipe and then reading the pipe, and there is no bound on how long it takes to answer.
/// [`Clipboard::read`] therefore starts the transfer on a thread of its own and the answer arrives
/// later as a wake — which is the shape the contract already has and the only shape this desktop
/// can honestly satisfy.
///
/// [`Clipboard::read_blocking`] still answers where it can. Two cases where it can: this
/// application owns the selection, in which case the value is here and no process has to be asked
/// at all; and another process answers within a quarter of a second, which is what this project
/// elsewhere calls a human-visible wait. Past that it refuses and the caller falls back to the
/// request, where the wait is on a thread nobody is watching.
///
/// # What crosses
///
/// Plain text and no more, which is what the contract promises and what the portable backend also
/// does. A request for markup, an image or a file list is refused with a reason a caller can fall
/// back from, rather than answered with an empty value it cannot tell from an empty clipboard.
#[derive(Debug)]
pub struct WaylandClipboard {
    /// Identifiers for reads answered through the loop.
    serials: ClipboardSerials,
    /// The devices, the offers, and what this application owns.
    selections: Arc<Selections>,
}

impl Default for WaylandClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl WaylandClipboard {
    /// A clipboard with no devices yet, which every request refuses until one arrives.
    pub fn new() -> Self {
        Self {
            serials: ClipboardSerials::new(),
            selections: Arc::new(Selections::default()),
        }
    }

    /// The devices and offers, which the loop keeps up to date.
    pub const fn selections(&self) -> &Arc<Selections> {
        &self.selections
    }
}

impl Clipboard for WaylandClipboard {
    fn read(&self, kind: ClipboardKind, format: ClipboardFormat) -> ClipboardSerial {
        let serial = self.serials.take();
        if !mime::is_supported(format) {
            self.selections
                .answer(serial, Err(ClipboardError::UnsupportedFormat(format)));
            return serial;
        }
        // Owned by this application: the value is here, and asking a process for what it already
        // has would be a round trip to itself — which on a single connection is a deadlock.
        if let Some(held) = self.selections.owned(kind) {
            self.selections
                .answer(serial, Ok(ClipboardData::Text(held)));
            return serial;
        }
        match self.selections.start_read(kind) {
            Ok(pending) => pending.answer_on_a_thread(Arc::clone(&self.selections), serial),
            Err(error) => self.selections.answer(serial, Err(error)),
        }
        serial
    }

    fn read_blocking(
        &self,
        kind: ClipboardKind,
        format: ClipboardFormat,
    ) -> Result<ClipboardData, ClipboardError> {
        if !mime::is_supported(format) {
            return Err(ClipboardError::UnsupportedFormat(format));
        }
        if let Some(held) = self.selections.owned(kind) {
            return Ok(ClipboardData::Text(held));
        }
        // Bounded by what a person would notice, because the thread waiting here is the one that
        // also reads input. Past the bound the caller is told to ask again through the loop.
        self.selections
            .start_read(kind)?
            .answer_now(pipe::IMPATIENCE)
    }

    fn write(
        &self,
        kind: ClipboardKind,
        data: ClipboardData,
        options: ClipboardWriteOptions,
    ) -> Result<(), ClipboardError> {
        let ClipboardData::Text(text) = data else {
            return Err(ClipboardError::UnsupportedFormat(data.format()));
        };
        // Nothing here can honour a request to keep a value out of a clipboard manager's history:
        // the protocol has no way to say it, and the managers that exist read every selection.
        // Recorded rather than silently ignored, so that a password field's copy can be found.
        if options.exclude_from_history {
            tracing::debug!(
                "this desktop has no way to keep a selection out of a clipboard manager's history"
            );
        }
        self.selections.take_selection(kind, Some(text))
    }

    fn clear(&self, kind: ClipboardKind) -> Result<(), ClipboardError> {
        self.selections.take_selection(kind, None)
    }
}

/// Where a clipboard answer goes.
pub(crate) fn deliver(
    waker: Option<&Arc<dyn zgui_platform::Waker>>,
    serial: ClipboardSerial,
    result: Result<ClipboardData, ClipboardError>,
) {
    let Some(waker) = waker else {
        tracing::warn!("a clipboard read was started before the loop could answer it");
        return;
    };
    waker.wake(WakeReason::ClipboardRead { serial, result });
}
