//! Polling the UI thread's task pool inside the runtime.

use tokio::runtime::Handle;
use zgui_reactive::PollContext;

/// Enters the runtime around each poll of the reactive task pool.
///
/// One `enter` per flush, not per task: the guard is ambient thread-local state, so everything the
/// flush polls sees it. That is what lets a UI-thread task construct a `tokio::time::Sleep` or a
/// socket at all — those register with the runtime's driver when they are *created*, which happens
/// inside the poll of the task that awaits them.
pub(crate) struct Entered {
    /// The runtime to enter.
    handle: Handle,
}

impl Entered {
    /// Wraps `handle`.
    pub(crate) fn new(handle: Handle) -> Self {
        Self { handle }
    }
}

impl PollContext for Entered {
    fn enter(&self, poll: &mut dyn FnMut()) {
        let _guard = self.handle.enter();
        poll();
    }
}
