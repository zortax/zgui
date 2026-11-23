//! The driver zgui's tests run a compositor behind.

use compositor_api::{Compositor, CompositorClient, Renderer, RendererFactory};
use compositor_driver::{DriverOptions, InProcess};
use compositor_thread::{ClientHandle, ThreadedCompositor};

use crate::select::DriverKind;
use crate::synced::Synced;

/// Returns `compositor` behind the driver `ZGUI_DRIVER` selects, stepped at the clocks the window
/// steps it at.
///
/// - `local`, the default, runs it on the test's own thread.
/// - `threaded` runs it on a thread of its own, and waits for the driver after every step, so a
///   test reads each frame as it would in process.
/// - `paced` runs it on a thread of its own at the display's rate, see [`self_paced`].
///
/// # Panics
///
/// Panics when the variable holds an unknown value.
pub fn driver(compositor: Box<dyn Compositor>) -> Box<dyn CompositorClient> {
    let kind = DriverKind::from_environment().unwrap_or_else(|why| panic!("no driver: {why}"));
    selected(kind, compositor)
}

/// Returns `compositor` behind the driver `ZGUI_DRIVER` selects, as [`driver`] does, except that
/// `threaded` returns from each step without waiting for the driver.
///
/// The [`Waiter`] waits for a driver on a thread of its own.
///
/// # Panics
///
/// Panics when the variable holds an unknown value.
pub fn unsynced_driver(compositor: Box<dyn Compositor>) -> (Box<dyn CompositorClient>, Waiter) {
    let kind = DriverKind::from_environment().unwrap_or_else(|why| panic!("no driver: {why}"));
    match kind {
        DriverKind::Local => (selected(kind, compositor), Waiter(None)),
        DriverKind::Threaded => unsynced_thread(compositor),
        DriverKind::Paced => {
            let (owner, handle) = ThreadedCompositor::spawn(compositor);
            (Box::new(owner), Waiter(Some(handle)))
        }
    }
}

/// Returns `compositor` on a thread of its own, stepped at the clocks the window steps it at, and
/// returning from each step without waiting for the driver.
///
/// The [`Waiter`] waits for the driver.
pub fn unsynced_thread(compositor: Box<dyn Compositor>) -> (Box<dyn CompositorClient>, Waiter) {
    let (owner, handle) = ThreadedCompositor::spawn_with(compositor, DriverOptions::stepped());
    (Box::new(owner), Waiter(Some(handle)))
}

/// A wait for a driver on a thread of its own.
pub struct Waiter(Option<ClientHandle>);

impl Waiter {
    /// Waits until the driver has applied every call queued before this one and published what
    /// they caused.
    ///
    /// A driver on the caller's own thread has nothing to wait for.
    pub fn wait(&mut self) {
        if let Some(handle) = &mut self.0 {
            handle.sync();
        }
    }
}

/// Returns `compositor` behind the driver of `kind`, a threaded one waited for after every step.
fn selected(kind: DriverKind, compositor: Box<dyn Compositor>) -> Box<dyn CompositorClient> {
    match kind {
        DriverKind::Local => Box::new(InProcess::new(compositor, DriverOptions::stepped())),
        DriverKind::Threaded => {
            let (owner, _handle) =
                ThreadedCompositor::spawn_with(compositor, DriverOptions::stepped());
            Box::new(Synced(owner))
        }
        DriverKind::Paced => self_paced(compositor),
    }
}

/// Returns `compositor` on a thread of its own that builds its frames at the display's rate.
pub fn self_paced(compositor: Box<dyn Compositor>) -> Box<dyn CompositorClient> {
    Box::new(ThreadedCompositor::spawn(compositor).0)
}

/// Returns a factory of the renderer `make` builds, on the thread that draws the output.
pub fn factory<R>(make: impl FnOnce() -> R + Send + 'static) -> RendererFactory
where
    R: Renderer + 'static,
{
    Box::new(move || Ok(Box::new(make())))
}
