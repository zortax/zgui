//! Work the page runs later.

use std::future::Future;

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// Runs `future` on the page's thread, between its other callbacks.
pub fn spawn_local(future: impl Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(future);
}

/// Runs `work` in a task of its own, after the current callback returns and the page had a chance
/// to paint.
///
/// A microtask runs before the page paints, so long work in one delays the frame that asked for
/// it. A task does not.
pub fn defer(work: impl FnOnce() + 'static) {
    let Some(window) = web_sys::window() else {
        work();
        return;
    };
    let callback = Closure::once_into_js(work);
    if window
        .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 0)
        .is_err()
    {
        tracing::warn!(target: "zgui::platform", "the page refused a deferred task");
    }
}

/// Writes `message` to the page's console as an error.
pub fn report(message: &str) {
    web_sys::console::error_1(&message.into());
}
