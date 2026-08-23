//! What is on each clipboard, and what this application has put there.

use std::sync::{Arc, Mutex, MutexGuard};

use smithay_client_toolkit::data_device_manager::DataDeviceManagerState;
use smithay_client_toolkit::data_device_manager::data_device::DataDevice;
use smithay_client_toolkit::data_device_manager::data_source::CopyPasteSource;
use smithay_client_toolkit::data_device_manager::{ReadPipe, WritePipe};
use smithay_client_toolkit::primary_selection::PrimarySelectionManagerState;
use smithay_client_toolkit::primary_selection::device::PrimarySelectionDevice;
use smithay_client_toolkit::primary_selection::selection::PrimarySelectionSource;
use smithay_client_toolkit::reexports::client::Connection;
use smithay_client_toolkit::reexports::client::QueueHandle;
use smithay_client_toolkit::reexports::client::protocol::wl_data_source::WlDataSource;
use smithay_client_toolkit::reexports::protocols::wp::primary_selection::zv1::client::zwp_primary_selection_source_v1::ZwpPrimarySelectionSourceV1;
use zgui_platform::{ClipboardData, ClipboardError, ClipboardKind, ClipboardSerial, Waker};

use crate::clipboard::{deliver, mime, pipe};
use crate::driver::WaylandState;

/// The devices, the offers on them, and what this application owns.
///
/// Shared and thread-safe because a read is answered on a thread of its own and a write can be
/// asked for from anywhere. The loop writes the device half; every request reads it.
#[derive(Debug, Default)]
pub struct Selections {
    /// Everything under one lock, because a read consults all of it at once.
    inner: Mutex<Inner>,
}

/// The changing half.
#[derive(Default)]
struct Inner {
    /// The connection every request is flushed on.
    conn: Option<Connection>,
    /// The queue the sources this application makes are dispatched on.
    qh: Option<QueueHandle<WaylandState>>,
    /// Where an answer to a read is delivered.
    waker: Option<Arc<dyn Waker>>,
    /// The standard clipboard's device and manager.
    standard: Option<Standard>,
    /// The selection clipboard's, where the compositor offers one.
    primary: Option<Primary>,
    /// The serial a claim on a selection may quote, which is always from a press.
    serial: Option<u32>,
    /// What this application has put on each clipboard.
    held: [Option<Held>; 2],
}

/// The ordinary clipboard's objects.
#[derive(Debug)]
struct Standard {
    /// The factory for sources.
    manager: DataDeviceManagerState,
    /// This seat's device.
    device: DataDevice,
}

/// The selection clipboard's objects.
#[derive(Debug)]
struct Primary {
    /// The factory for sources.
    manager: PrimarySelectionManagerState,
    /// This seat's device.
    device: PrimarySelectionDevice,
}

/// A selection this application owns, and the source that serves it.
#[derive(Debug)]
struct Held {
    /// What was put there.
    text: zgui_vocab::SharedString,
    /// The source serving it, kept alive for as long as the selection is owned.
    source: Source,
}

/// The source object, whichever clipboard it belongs to.
#[derive(Debug)]
enum Source {
    /// A source on the ordinary clipboard.
    Standard(CopyPasteSource),
    /// A source on the selection clipboard.
    Primary(PrimarySelectionSource),
}

/// A read that has been asked for and not yet answered.
#[derive(Debug)]
pub struct Pending {
    /// The pipe the other process writes into.
    reader: ReadPipe,
    /// Which clipboard it came from, for the error that says it was empty.
    kind: ClipboardKind,
}

impl Pending {
    /// Waits for the answer here, up to `patience`.
    pub fn answer_now(
        self,
        patience: std::time::Duration,
    ) -> Result<ClipboardData, ClipboardError> {
        finish(self.kind, pipe::read(self.reader, patience))
    }

    /// Waits for the answer on a thread of its own, and delivers it to the loop.
    ///
    /// A thread rather than a source on the loop, because the wait is on another process and the
    /// point of this backend is that the loop's own thread never waits on one.
    pub fn answer_on_a_thread(self, selections: Arc<Selections>, serial: ClipboardSerial) {
        let kind = self.kind;
        let answering = Arc::clone(&selections);
        let started = std::thread::Builder::new()
            .name("zgui-clipboard".to_owned())
            .spawn(move || {
                let answer = finish(kind, pipe::read(self.reader, pipe::PATIENCE));
                answering.answer(serial, answer);
            });
        if let Err(error) = started {
            // Nobody is going to answer, so the request is answered here instead: a read that is
            // never answered leaves whatever asked for it waiting for ever.
            selections.answer(serial, Err(ClipboardError::Backend(error.to_string())));
        }
    }
}

/// What a completed transfer means.
fn finish(
    kind: ClipboardKind,
    taken: Result<Vec<u8>, pipe::Failed>,
) -> Result<ClipboardData, ClipboardError> {
    let bytes = taken?;
    if bytes.is_empty() {
        return Err(ClipboardError::Empty(kind));
    }
    // Lossy rather than refused: a selection whose owner declared UTF-8 and wrote something else
    // is still mostly readable, and refusing the paste entirely helps nobody.
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Ok(ClipboardData::Text(text.into()))
}

impl core::fmt::Debug for Inner {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Selections")
            .field("standard", &self.standard.is_some())
            .field("primary", &self.primary.is_some())
            .field(
                "owns",
                &self.held.iter().filter(|held| held.is_some()).count(),
            )
            .finish_non_exhaustive()
    }
}

impl Selections {
    /// Records the connection and the queue everything is made on.
    pub(crate) fn attach(
        &self,
        conn: Connection,
        qh: QueueHandle<WaylandState>,
        waker: Arc<dyn Waker>,
    ) {
        let mut inner = self.lock();
        inner.conn = Some(conn);
        inner.qh = Some(qh);
        inner.waker = Some(waker);
    }

    /// Records this seat's devices.
    pub(crate) fn devices(
        &self,
        standard: Option<(DataDeviceManagerState, DataDevice)>,
        primary: Option<(PrimarySelectionManagerState, PrimarySelectionDevice)>,
    ) {
        let mut inner = self.lock();
        inner.standard = standard.map(|(manager, device)| Standard { manager, device });
        inner.primary = primary.map(|(manager, device)| Primary { manager, device });
    }

    /// Records the serial a claim on a selection may quote.
    ///
    /// Any input serial, not only a press. A pop-up grab and an interactive drag are declined
    /// against anything but a press, but a selection is not: the protocol asks for the serial of
    /// the event that caused the request, and a copy caused by the window merely being focused —
    /// a menu command, a shortcut on a freshly focused window — has only that one to give. Being
    /// stricter here means a copy that silently does nothing until the user has clicked.
    pub(crate) fn observed(&self, serial: u32) {
        self.lock().serial = Some(serial);
    }

    /// What this application has put on `kind`, when it is the owner.
    pub fn owned(&self, kind: ClipboardKind) -> Option<zgui_vocab::SharedString> {
        self.lock().held[index(kind)]
            .as_ref()
            .map(|held| held.text.clone())
    }

    /// Gives up ownership of `kind`, because the compositor gave it to somebody else.
    pub(crate) fn lost(&self, source: &WlDataSource) {
        let mut inner = self.lock();
        for held in &mut inner.held {
            if matches!(held.as_ref().map(|held| &held.source), Some(Source::Standard(owned)) if owned.inner() == source)
            {
                *held = None;
            }
        }
    }

    /// The same, for the selection clipboard.
    pub(crate) fn lost_primary(&self, source: &ZwpPrimarySelectionSourceV1) {
        let mut inner = self.lock();
        for held in &mut inner.held {
            if matches!(held.as_ref().map(|held| &held.source), Some(Source::Primary(owned)) if owned.inner() == source)
            {
                *held = None;
            }
        }
    }

    /// Serves a request for what this application put on a clipboard.
    ///
    /// On a thread, because the destination is another process and a destination that stops
    /// reading would otherwise hold the loop for as long as it liked.
    pub(crate) fn serve(&self, source: &WlDataSource, destination: WritePipe) {
        let text = self.lock().held.iter().flatten().find_map(|held| {
            matches!(&held.source, Source::Standard(owned) if owned.inner() == source)
                .then(|| held.text.clone())
        });
        serve_bytes(text, destination);
    }

    /// The same, for the selection clipboard.
    pub(crate) fn serve_primary(
        &self,
        source: &ZwpPrimarySelectionSourceV1,
        destination: WritePipe,
    ) {
        let text = self.lock().held.iter().flatten().find_map(|held| {
            matches!(&held.source, Source::Primary(owned) if owned.inner() == source)
                .then(|| held.text.clone())
        });
        serve_bytes(text, destination);
    }

    /// Asks the owner of `kind` for its text, answering with the pipe it will write into.
    pub(crate) fn start_read(&self, kind: ClipboardKind) -> Result<Pending, ClipboardError> {
        let inner = self.lock();
        let reader = match kind {
            ClipboardKind::Primary => {
                let primary = inner
                    .primary
                    .as_ref()
                    .ok_or(ClipboardError::UnsupportedKind(kind))?;
                let offer = primary
                    .device
                    .data()
                    .selection_offer()
                    .ok_or(ClipboardError::Empty(kind))?;
                let wanted = offer.with_mime_types(mime::best_text).ok_or(
                    ClipboardError::UnavailableFormat(zgui_platform::ClipboardFormat::Text),
                )?;
                offer
                    .receive(wanted)
                    .map_err(|error| ClipboardError::Backend(error.to_string()))?
            }
            _ => {
                let standard = inner.standard.as_ref().ok_or(ClipboardError::Empty(kind))?;
                let offer = standard
                    .device
                    .data()
                    .selection_offer()
                    .ok_or(ClipboardError::Empty(kind))?;
                let wanted = offer.with_mime_types(mime::best_text).ok_or(
                    ClipboardError::UnavailableFormat(zgui_platform::ClipboardFormat::Text),
                )?;
                offer
                    .receive(wanted)
                    .map_err(|error| ClipboardError::Backend(error.to_string()))?
            }
        };
        // The request has to reach the compositor before anything waits on the pipe, and the loop
        // is not going to flush until it next parks — which is after this returns.
        if let Some(conn) = &inner.conn {
            let _ = conn.flush();
        }
        Ok(Pending { reader, kind })
    }

    /// Claims `kind` for this application, serving `text`, or gives it up when there is none.
    pub(crate) fn take_selection(
        &self,
        kind: ClipboardKind,
        text: Option<zgui_vocab::SharedString>,
    ) -> Result<(), ClipboardError> {
        let mut inner = self.lock();
        let serial = inner.serial.ok_or_else(|| {
            // A compositor grants a selection only against a serial from something the user did.
            // Before the first press there is nothing to quote, and claiming without one is
            // declined silently — so it is reported rather than appearing to work.
            ClipboardError::Backend(
                "a selection can only be claimed after the user has pressed something".to_owned(),
            )
        })?;
        let qh = inner
            .qh
            .clone()
            .ok_or_else(|| ClipboardError::Backend("the loop has not started".to_owned()))?;

        let held = match (kind, text) {
            (_, None) => {
                match kind {
                    ClipboardKind::Primary => {
                        if let Some(primary) = &inner.primary {
                            primary.device.unset_selection(serial);
                        }
                    }
                    _ => {
                        if let Some(standard) = &inner.standard {
                            standard.device.unset_selection(serial);
                        }
                    }
                }
                None
            }
            (ClipboardKind::Primary, Some(text)) => {
                let primary = inner
                    .primary
                    .as_ref()
                    .ok_or(ClipboardError::UnsupportedKind(kind))?;
                let source = primary.manager.create_selection_source(&qh, mime::OFFERED);
                source.set_selection(&primary.device, serial);
                Some(Held {
                    text,
                    source: Source::Primary(source),
                })
            }
            (_, Some(text)) => {
                let standard = inner.standard.as_ref().ok_or_else(|| {
                    ClipboardError::Backend("this compositor offers no clipboard".to_owned())
                })?;
                let source = standard
                    .manager
                    .create_copy_paste_source(&qh, mime::OFFERED);
                source.set_selection(&standard.device, serial);
                Some(Held {
                    text,
                    source: Source::Standard(source),
                })
            }
        };
        // Replaced rather than dropped first: dropping the old source destroys it, and destroying
        // the source that currently owns the selection before the new one has claimed it leaves a
        // gap in which the clipboard is empty.
        inner.held[index(kind)] = held;
        if let Some(conn) = &inner.conn {
            let _ = conn.flush();
        }
        Ok(())
    }

    /// Delivers an answer to whoever asked for it.
    pub(crate) fn answer(
        &self,
        serial: ClipboardSerial,
        result: Result<ClipboardData, ClipboardError>,
    ) {
        let waker = self.lock().waker.clone();
        deliver(waker.as_ref(), serial, result);
    }

    /// The state, recovering from a panic on another thread.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Writes a held selection into `destination`, on a thread.
fn serve_bytes(text: Option<zgui_vocab::SharedString>, destination: WritePipe) {
    let bytes = text.map(|text| text.as_str().as_bytes().to_vec());
    let _ = std::thread::Builder::new()
        .name("zgui-clipboard-write".to_owned())
        .spawn(move || {
            // A destination asking for a selection this application no longer holds gets an empty
            // one: closing the pipe with nothing in it is how the protocol says "nothing here".
            pipe::write(
                destination,
                bytes.as_deref().unwrap_or_default(),
                pipe::PATIENCE,
            );
        });
}

/// Which slot a clipboard's state is kept in.
const fn index(kind: ClipboardKind) -> usize {
    match kind {
        ClipboardKind::Primary => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::{finish, index};
    use crate::clipboard::pipe::Failed;
    use zgui_platform::{ClipboardData, ClipboardError, ClipboardKind};

    #[test]
    fn the_two_clipboards_are_kept_apart() {
        // Writing the selection clipboard by mistake destroys whatever the user had selected
        // elsewhere, so the two must never share a slot.
        assert_ne!(
            index(ClipboardKind::Standard),
            index(ClipboardKind::Primary)
        );
    }

    #[test]
    fn an_empty_transfer_is_an_empty_clipboard_rather_than_a_failure() {
        assert_eq!(
            finish(ClipboardKind::Standard, Ok(Vec::new())),
            Err(ClipboardError::Empty(ClipboardKind::Standard))
        );
    }

    #[test]
    fn what_arrived_is_what_is_answered_with() {
        assert_eq!(
            finish(ClipboardKind::Standard, Ok(b"copied".to_vec())),
            Ok(ClipboardData::Text("copied".into()))
        );
    }

    #[test]
    fn text_that_is_not_valid_is_read_as_far_as_it_goes_rather_than_refused() {
        // A selection whose owner declared UTF-8 and wrote something else is still mostly
        // readable, and refusing the paste entirely helps nobody.
        let answered = finish(ClipboardKind::Standard, Ok(vec![b'o', b'k', 0xff]));
        assert!(matches!(answered, Ok(ClipboardData::Text(_))));
    }

    #[test]
    fn an_owner_that_never_answered_is_reported_as_a_timeout() {
        assert_eq!(
            finish(ClipboardKind::Primary, Err(Failed::TimedOut)),
            Err(ClipboardError::TimedOut)
        );
    }
}
