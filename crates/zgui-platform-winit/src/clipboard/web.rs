//! Plain-text clipboard access through the browser's asynchronous API.

use std::cell::RefCell;
use std::sync::Arc;

use raw_window_handle::HasDisplayHandle;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use zgui_platform::{
    Clipboard, ClipboardData, ClipboardError, ClipboardFormat, ClipboardKind, ClipboardSerial,
    ClipboardSerials, ClipboardWriteOptions, WakeReason, Waker,
};

/// The browser clipboard, using the same platform API as the desktop adapter.
///
/// Reads complete through the event-loop waker. Writes report whether the request was submitted;
/// browser permission failures that arrive later are reported to the console.
pub struct DesktopClipboard {
    /// Identifiers paired with asynchronous read results.
    serials: ClipboardSerials,
    /// Where the browser's promise completion is delivered.
    waker: RefCell<Option<Arc<dyn Waker>>>,
}

impl Default for DesktopClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopClipboard {
    /// Creates a clipboard before the event loop is attached.
    pub fn new() -> Self {
        Self {
            serials: ClipboardSerials::new(),
            waker: RefCell::new(None),
        }
    }

    /// Connects promise completions to the application's event loop.
    pub(crate) fn attach(&self, _display: &dyn HasDisplayHandle, waker: Arc<dyn Waker>) {
        *self.waker.borrow_mut() = Some(waker);
    }
}

impl core::fmt::Debug for DesktopClipboard {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("BrowserClipboard")
    }
}

impl Clipboard for DesktopClipboard {
    fn read(&self, kind: ClipboardKind, format: ClipboardFormat) -> ClipboardSerial {
        let serial = self.serials.take();
        let waker = self.waker.borrow().clone();
        let request = validate(kind, format).and_then(|()| browser_clipboard());
        // Start the browser operation in the originating event's turn, before yielding.
        let request = request.map(|clipboard| clipboard.read_text());
        spawn_local(async move {
            let result = match request {
                Ok(promise) => JsFuture::from(promise)
                    .await
                    .map_err(backend_error)
                    .map(|text| ClipboardData::Text(text.as_string().unwrap_or_default().into())),
                Err(error) => Err(error),
            };
            if let Some(waker) = waker {
                waker.wake(WakeReason::ClipboardRead { serial, result });
            }
        });
        serial
    }

    fn read_blocking(
        &self,
        kind: ClipboardKind,
        format: ClipboardFormat,
    ) -> Result<ClipboardData, ClipboardError> {
        validate(kind, format)?;
        Err(ClipboardError::Backend(
            "the browser clipboard requires an asynchronous read".to_owned(),
        ))
    }

    fn write(
        &self,
        kind: ClipboardKind,
        data: ClipboardData,
        _options: ClipboardWriteOptions,
    ) -> Result<(), ClipboardError> {
        validate(kind, data.format())?;
        let text = data
            .as_text()
            .ok_or(ClipboardError::UnsupportedFormat(data.format()))?;
        let promise = browser_clipboard()?.write_text(text);
        spawn_local(async move {
            if let Err(error) = JsFuture::from(promise).await {
                web_sys::console::warn_2(&"clipboard write failed".into(), &error);
            }
        });
        Ok(())
    }

    fn clear(&self, kind: ClipboardKind) -> Result<(), ClipboardError> {
        self.write(
            kind,
            ClipboardData::from(""),
            ClipboardWriteOptions::default(),
        )
    }
}

/// The one clipboard and representation browsers expose here.
fn validate(kind: ClipboardKind, format: ClipboardFormat) -> Result<(), ClipboardError> {
    if kind != ClipboardKind::Standard {
        return Err(ClipboardError::UnsupportedKind(kind));
    }
    if format != ClipboardFormat::Text {
        return Err(ClipboardError::UnsupportedFormat(format));
    }
    Ok(())
}

/// Finds the clipboard API in a browser context that exposes it.
fn browser_clipboard() -> Result<web_sys::Clipboard, ClipboardError> {
    let window =
        web_sys::window().ok_or_else(|| ClipboardError::Backend("no browser window".to_owned()))?;
    let clipboard = window.navigator().clipboard();
    let value: &JsValue = clipboard.as_ref();
    if value.is_undefined() || value.is_null() {
        return Err(ClipboardError::Backend(
            "the browser has no clipboard API".to_owned(),
        ));
    }
    Ok(clipboard)
}

/// Preserves the browser's error message when it provides one.
fn backend_error(error: JsValue) -> ClipboardError {
    ClipboardError::Backend(error.as_string().unwrap_or_else(|| format!("{error:?}")))
}
