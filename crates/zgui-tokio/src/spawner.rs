//! Routing zgui's background work to a tokio runtime.

use std::future::Future;
use std::pin::Pin;

use tokio::runtime::Handle;
use zgui_reactive::BackgroundSpawner;

/// zgui's background executor, as a tokio runtime.
pub(crate) struct TokioSpawner {
    /// The runtime the work goes to.
    handle: Handle,
}

impl TokioSpawner {
    /// Wraps `handle`.
    pub(crate) fn new(handle: Handle) -> Self {
        Self { handle }
    }
}

impl BackgroundSpawner for TokioSpawner {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        // The join handle is dropped, which detaches rather than cancels. Cancellation is the
        // awaiting UI-thread task's business: it holds a `Task` that its scope cancels, and the
        // oneshot it is waiting on is what actually goes away.
        drop(self.handle.spawn(future));
    }

    fn spawn_blocking(&self, work: Box<dyn FnOnce() + Send>) {
        // tokio's blocking pool, which is separate from its worker threads — the thing the default
        // executor in `zgui-reactive` does not have, and the reason a program doing real blocking
        // work should install this crate.
        drop(self.handle.spawn_blocking(work));
    }
}
