//! Clipboard deliberately unavailable during Android bring-up.

use zgui_platform::{ClipboardError, ClipboardKind};

#[derive(Default)]
pub(crate) struct Desktop;

impl Desktop {
    pub(crate) fn text(&mut self, kind: ClipboardKind) -> Result<String, ClipboardError> {
        Err(ClipboardError::UnsupportedKind(kind))
    }

    pub(crate) fn set_text(
        &mut self,
        kind: ClipboardKind,
        _text: String,
        _secret: bool,
    ) -> Result<(), ClipboardError> {
        Err(ClipboardError::UnsupportedKind(kind))
    }
}
