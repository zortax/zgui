//! A graphics device under the gallery, drawing to a texture instead of to a window.
//!
//! The compositor draws everything zgui records onto its reference renderer, see [`compositor`].
//! Only the destination differs from an opened window, because a window's surface belongs to the
//! platform's compositor the instant it is presented and cannot be read back.

#![allow(
    dead_code,
    unreachable_pub,
    reason = "one support module serves several groups of assertions, none of which uses all of it"
)]

pub mod compositor;
pub mod frame;
pub mod ink;
pub mod picture;
pub mod shot;

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crate::device::frame::Frame;

/// Serialises every fixture in this binary onto the device.
///
/// A process has one graphics device and one reactive runtime per thread; these fixtures are the
/// only thing that would ask for several of either at once.
pub fn device_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|held| held.into_inner())
}

/// Every frame of one run, in the order they were drawn.
pub type Log = Arc<Mutex<Vec<Frame>>>;

/// Whether frames are drawn with the damage the application asked for.
static REAL_DAMAGE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Draws every frame with the application's own damage set from now on.
///
/// The default redraws the whole window every frame, which answers what the document *contains*.
/// A fixture asking what a partial repaint left on the screen wants the real damage, and reads
/// the composed target rather than the presented one.
#[allow(dead_code, reason = "not every fixture asks for it")]
pub fn use_real_damage() {
    REAL_DAMAGE.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// How many frames moved composed pixels in place of drawing a scrolled port.
pub(crate) static SHIFTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many frames so far answered a scroll by moving composed pixels.
#[allow(dead_code, reason = "not every fixture scrolls")]
pub fn shifts() -> usize {
    SHIFTS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Reports whether frames are drawn with the application's own damage set.
pub fn real_damage() -> bool {
    REAL_DAMAGE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Returns `app` drawing every window through the compositor onto the device, with each frame
/// written into `log`.
pub fn drawn_on(app: zgui::App, log: &Log) -> zgui::App {
    app.with_compositor_client(Box::new(zgui_testkit_compositor::driver))
        .with_compositor_renderer(compositor::factory(log))
        .with_probe(compositor::repainting())
}

/// Reports whether this machine has a graphics device these fixtures can run on.
pub fn available() -> bool {
    compositor::available()
}
