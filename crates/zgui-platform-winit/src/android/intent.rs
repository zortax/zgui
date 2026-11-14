//! Intents that open the application: links, shares, notification and widget taps.
//! Needs `ZguiActivity`.

use super::jvm;
use std::sync::Mutex;

/// An intent that opened or reached the application.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Intent {
    /// Intent action, such as `android.intent.action.VIEW` or `android.intent.action.SEND`.
    pub action: Option<String>,
    /// Data URI, such as a link that opened the application.
    pub data: Option<String>,
    /// Shared text.
    pub text: Option<String>,
    /// Shared file as a content URI; read it with [`document::reopen`](super::document::reopen).
    pub stream: Option<String>,
    /// MIME type.
    pub mime: Option<String>,
    /// Payload of a notification, shortcut or widget tap.
    pub payload: Option<String>,
}

type Handler = Box<dyn Fn(Intent) + Send>;
static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);

/// Sends each intent to `callback`, starting with the one that launched the activity.
pub fn on_intent(callback: impl Fn(Intent) + Send + 'static) {
    *HANDLER.lock().expect("Android intent lock") = Some(Box::new(callback));
    arrived();
}

/// Delivers queued intents. Java queues each intent, then signals.
pub(crate) fn arrived() {
    let handler = HANDLER.lock().expect("Android intent lock");
    let Some(handler) = handler.as_ref() else {
        return;
    };
    let fields = match jvm::call("intents", 0, &[]) {
        Ok(fields) => fields,
        Err(error) => {
            tracing::warn!(%error, "Android intents unavailable");
            return;
        }
    };
    let some = |value: &String| (!value.is_empty()).then(|| value.clone());
    for item in fields.chunks_exact(6) {
        handler(Intent {
            action: some(&item[0]),
            data: some(&item[1]),
            text: some(&item[2]),
            stream: some(&item[3]),
            mime: some(&item[4]),
            payload: some(&item[5]),
        });
    }
}
