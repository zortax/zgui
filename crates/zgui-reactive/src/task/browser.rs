//! Browser tasks, scheduled on the browser's main thread.
//!
//! This initial backend uses JavaScript microtasks. It creates no workers. Synchronous CPU work
//! runs on the UI thread and delays input and painting until it returns; applications must keep
//! such work small or install a worker-backed executor.

use std::future::Future;
use std::pin::Pin;

use crate::task::background::BackgroundSpawner;

/// The single-thread browser executor.
pub(crate) struct Browser;

impl BackgroundSpawner for Browser {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        wasm_bindgen_futures::spawn_local(future);
    }

    fn spawn_blocking(&self, work: Box<dyn FnOnce() + Send>) {
        // Deferring avoids re-entering the frame that requested the work. The closure still runs
        // synchronously on the browser main thread when the microtask is polled.
        wasm_bindgen_futures::spawn_local(async move { work() });
    }
}
