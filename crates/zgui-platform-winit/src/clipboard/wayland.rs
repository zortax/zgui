//! The clipboard as a Wayland compositor offers it.

use std::ffi::c_void;

use zgui_platform::{ClipboardError, ClipboardKind};

/// The Wayland clipboard, over the connection the window system already owns.
///
/// This is a second implementation rather than a configuration of the first, and the reason is
/// protocol rather than taste. The desktop-wide clipboard library speaks the data-control protocol
/// on Wayland, which is the protocol screen recorders and clipboard managers use: it is not
/// available on every compositor, it needs a privileged interface where it is, and a windowed
/// application is not what it is for. This one speaks the ordinary data-device protocol over the
/// application's own connection, which is what a window is supposed to use and what works
/// everywhere.
///
/// It runs on a thread of its own with its own queue, because a clipboard request is answered by
/// another process and may take as long as that process likes.
pub(crate) struct Wayland {
    /// The connection-owning worker.
    clipboard: smithay_clipboard::Clipboard,
}

impl Wayland {
    /// A clipboard over the compositor connection `display` points at.
    ///
    /// # Safety
    ///
    /// `display` must be a live `wl_display` pointer that outlives the returned value.
    pub(crate) unsafe fn new(display: *mut c_void) -> Self {
        // SAFETY: the caller has promised the pointer is a live `wl_display` that outlives this
        // value. Every call site takes it from the running event loop's own display handle and
        // stores the result beside the loop, so the connection outlives the clipboard by
        // construction.
        let clipboard = unsafe { smithay_clipboard::Clipboard::new(display) };
        Self { clipboard }
    }

    /// What is on `kind`, as text.
    pub(crate) fn text(&self, kind: ClipboardKind) -> Result<String, ClipboardError> {
        let loaded = match kind {
            ClipboardKind::Primary => self.clipboard.load_primary(),
            _ => self.clipboard.load(),
        };
        match loaded {
            Ok(text) if text.is_empty() => Err(ClipboardError::Empty(kind)),
            Ok(text) => Ok(text),
            // The owner of a selection can decline, or go away mid-request, and neither is a
            // reason to stop: a paste command that cannot read is greyed out, not fatal.
            Err(error) => Err(ClipboardError::Backend(error.to_string())),
        }
    }

    /// Puts `text` on `kind`.
    pub(crate) fn set_text(&self, kind: ClipboardKind, text: String) {
        match kind {
            ClipboardKind::Primary => self.clipboard.store_primary(text),
            _ => self.clipboard.store(text),
        }
    }
}
