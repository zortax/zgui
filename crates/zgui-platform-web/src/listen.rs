//! Event listeners that are removed when they are dropped.

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{AddEventListenerOptions, Event, EventTarget};

/// One listener on one target. Dropping it removes it from the target.
pub(crate) struct Listener {
    /// What is listened to.
    target: EventTarget,
    /// The event name.
    kind: &'static str,
    /// The callback the browser holds.
    callback: Closure<dyn FnMut(Event)>,
}

impl Listener {
    /// Listens for `kind` on `target`. The listener may cancel the event's default action.
    pub(crate) fn new(
        target: &EventTarget,
        kind: &'static str,
        handle: impl FnMut(Event) + 'static,
    ) -> Self {
        let callback = Closure::<dyn FnMut(Event)>::new(handle);
        let options = AddEventListenerOptions::new();
        options.set_passive(false);
        if target
            .add_event_listener_with_callback_and_add_event_listener_options(
                kind,
                callback.as_ref().unchecked_ref(),
                &options,
            )
            .is_err()
        {
            tracing::warn!(target: "zgui::platform", kind, "the page refused an event listener");
        }
        Self {
            target: target.clone(),
            kind,
            callback,
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self
            .target
            .remove_event_listener_with_callback(self.kind, self.callback.as_ref().unchecked_ref());
    }
}
