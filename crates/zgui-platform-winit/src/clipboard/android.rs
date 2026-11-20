//! Android clipboard through the Java activity host.

use zgui_platform::{ClipboardError, ClipboardKind};

#[derive(Default)]
pub(crate) struct Desktop {}

impl Desktop {
    pub(crate) fn text(&mut self, kind: ClipboardKind) -> Result<String, ClipboardError> {
        if kind != ClipboardKind::Standard {
            return Err(ClipboardError::UnsupportedKind(kind));
        }
        crate::android_host::clipboard(None).map_err(ClipboardError::Backend)
    }

    pub(crate) fn set_text(
        &mut self,
        kind: ClipboardKind,
        text: String,
        secret: bool,
    ) -> Result<(), ClipboardError> {
        if kind != ClipboardKind::Standard {
            return Err(ClipboardError::UnsupportedKind(kind));
        }
        crate::android_host::clipboard(Some((&text, secret)))
            .map(|_| ())
            .map_err(ClipboardError::Backend)
    }
}
