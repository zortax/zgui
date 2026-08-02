//! How another thread reaches a loop that is asleep.

use std::sync::Mutex;

use winit::event_loop::EventLoopProxy;
use zgui_platform::{WakeReason, Waker};

/// Something delivered to the loop from outside its own event stream.
///
/// Two things arrive this way and they are unrelated: work finishing on another thread, and the
/// accessibility channel's own connection asking for something. They share this type because the
/// loop has exactly one way in for anything that is not an event about a window, and keeping that
/// to one channel is what makes the wake path auditable.
#[derive(Debug)]
#[non_exhaustive]
pub enum UserEvent {
    /// Something outside the surfaces asked for attention.
    Wake(WakeReason),
    /// The accessibility adapter has something to say about one window.
    A11y(accesskit_winit::Event),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::A11y(event)
    }
}

/// A handle that wakes the loop from anywhere.
///
/// This is the only object this backend hands out that crosses threads, and it is what closes the
/// gap between work finishing somewhere else and a frame appearing. A value written on a worker
/// thread marks something dirty; nothing about that reaches a loop blocked on the compositor's
/// socket; so the write ends here, the loop is interrupted, and the reason is delivered on the
/// loop's own thread where it is safe to act on.
///
/// The proxy underneath is shareable but not usable from two threads at once, so it is kept behind
/// a lock. A wake is a pointer write and a byte down a pipe, so the lock is held for as long as
/// that takes and no longer.
pub struct ProxyWaker {
    /// The channel into the loop.
    proxy: Mutex<EventLoopProxy<UserEvent>>,
}

impl ProxyWaker {
    /// A waker that delivers into the loop `proxy` belongs to.
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            proxy: Mutex::new(proxy),
        }
    }
}

impl core::fmt::Debug for ProxyWaker {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("ProxyWaker")
    }
}

impl Waker for ProxyWaker {
    fn wake(&self, reason: WakeReason) {
        let proxy = self.proxy.lock().expect("the channel is not poisoned");
        // A wake sent to a loop that has already finished is discarded rather than reported: the
        // only thing a caller could do with the error is ignore it, and every caller would.
        if proxy.send_event(UserEvent::Wake(reason)).is_err() {
            tracing::debug!(target: "zgui::platform", "a wake arrived after the loop had finished");
        }
    }
}
