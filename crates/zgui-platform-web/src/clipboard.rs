//! The page's clipboard.
//!
//! A page reads its clipboard only through a promise, so a read here always answers later, through
//! the waker. A synchronous read answers the text this application wrote last, because that text
//! needs no promise.

use std::cell::RefCell;
use std::sync::Arc;

use wasm_bindgen_futures::JsFuture;
use zgui_platform::{
    Clipboard, ClipboardData, ClipboardError, ClipboardFormat, ClipboardKind, ClipboardSerial,
    ClipboardSerials, ClipboardWriteOptions, WakeReason, Waker,
};

/// The clipboard of one page, for plain text.
pub struct PageClipboard {
    /// The identifiers of asynchronous reads.
    serials: ClipboardSerials,
    /// Where an answer is delivered.
    waker: Arc<dyn Waker>,
    /// The text most recently written here or pasted into the page.
    known: RefCell<Option<String>>,
}

impl core::fmt::Debug for PageClipboard {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("PageClipboard")
    }
}

impl PageClipboard {
    /// A clipboard that answers through `waker`.
    pub(crate) fn new(waker: Arc<dyn Waker>) -> Self {
        Self {
            serials: ClipboardSerials::new(),
            waker,
            known: RefCell::new(None),
        }
    }

    /// Records text the page was given by a paste, which needs no permission to read.
    pub(crate) fn remember(&self, text: String) {
        *self.known.borrow_mut() = Some(text);
    }
}

impl Clipboard for PageClipboard {
    fn read(&self, kind: ClipboardKind, format: ClipboardFormat) -> ClipboardSerial {
        let serial = self.serials.take();
        let waker = Arc::clone(&self.waker);
        // The request starts inside the event that asked for it, because a browser grants a
        // clipboard read only while a user gesture is being handled.
        let request = supported(kind, format)
            .and_then(|()| page_clipboard())
            .map(|clipboard| clipboard.read_text());
        crate::task::spawn_local(async move {
            let result = match request {
                Ok(promise) => JsFuture::from(promise)
                    .await
                    .map(|text| ClipboardData::from(text.as_string().unwrap_or_default().as_str()))
                    .map_err(|error| ClipboardError::Backend(describe(&error))),
                Err(error) => Err(error),
            };
            waker.wake(WakeReason::ClipboardRead { serial, result });
        });
        serial
    }

    fn read_blocking(
        &self,
        kind: ClipboardKind,
        format: ClipboardFormat,
    ) -> Result<ClipboardData, ClipboardError> {
        supported(kind, format)?;
        self.known
            .borrow()
            .as_deref()
            .map(ClipboardData::from)
            .ok_or(ClipboardError::Empty(kind))
    }

    fn write(
        &self,
        kind: ClipboardKind,
        data: ClipboardData,
        _options: ClipboardWriteOptions,
    ) -> Result<(), ClipboardError> {
        supported(kind, data.format())?;
        let text = data
            .as_text()
            .ok_or(ClipboardError::UnsupportedFormat(data.format()))?;
        *self.known.borrow_mut() = Some(text.to_owned());
        let promise = page_clipboard()?.write_text(text);
        crate::task::spawn_local(async move {
            if let Err(error) = JsFuture::from(promise).await {
                tracing::warn!(
                    target: "zgui::platform",
                    reason = %describe(&error),
                    "the page refused a clipboard write"
                );
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

/// The one clipboard and the one representation a page offers here.
fn supported(kind: ClipboardKind, format: ClipboardFormat) -> Result<(), ClipboardError> {
    if kind != ClipboardKind::Standard {
        return Err(ClipboardError::UnsupportedKind(kind));
    }
    if format != ClipboardFormat::Text {
        return Err(ClipboardError::UnsupportedFormat(format));
    }
    Ok(())
}

/// The clipboard interface, which only a secure context has.
fn page_clipboard() -> Result<web_sys::Clipboard, ClipboardError> {
    let missing = || ClipboardError::Backend("this page has no clipboard interface".to_owned());
    let navigator = web_sys::window().ok_or_else(missing)?.navigator();
    let clipboard = js_sys::Reflect::get(&navigator, &"clipboard".into()).map_err(|_| missing())?;
    if clipboard.is_undefined() || clipboard.is_null() {
        return Err(missing());
    }
    Ok(navigator.clipboard())
}

/// The message of a rejected promise.
fn describe(error: &wasm_bindgen::JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            js_sys::Reflect::get(error, &"message".into())
                .ok()
                .and_then(|message| message.as_string())
        })
        .unwrap_or_else(|| format!("{error:?}"))
}
