//! What every property asserted against the real event loop needs.
//!
//! Each of these targets is its own program, and that is forced rather than chosen: a process may
//! create exactly one event loop, ever, and on most desktops it must be created on the process's
//! own first thread. So each property gets a binary, each binary runs one loop, and the assertions
//! are made after the loop has finished and the application can be read back.
//!
//! Not every program here needs every piece, so what one of them does not use is not dead code but
//! a piece another one needs.

#![allow(dead_code)]

use std::thread;
use std::time::Duration;

use zgui_platform::{PlatformError, Waker};
use zgui_platform_winit::UserEvent;

/// How long a scripted property is allowed to take before something is declared stuck.
///
/// A loop that parks when it should not is not slow — it never finishes at all — so a program
/// asserting on parking has to be able to say that out loud rather than hang a machine.
const PATIENCE: Duration = Duration::from_secs(20);

/// The event loop, or nothing on a machine with no windowing system.
///
/// Skipping rather than failing is deliberate, and so is saying so loudly: these programs assert
/// what a real loop does, and a machine with no display server has nothing to say about it. A
/// silent green run on such a machine would be indistinguishable from a pass.
pub(crate) fn event_loop() -> Option<winit::event_loop::EventLoop<UserEvent>> {
    match zgui_platform_winit::event_loop() {
        Ok(event_loop) => Some(event_loop),
        Err(PlatformError::Backend(reason)) => {
            eprintln!("SKIPPED: this machine has no windowing system to run a loop on: {reason}");
            None
        }
        Err(other) => {
            eprintln!("SKIPPED: the event loop could not be opened: {other}");
            None
        }
    }
}

/// Delivers `reason` to the loop after `delay`, from a thread of its own.
///
/// This is what a property about waking needs and what nothing inside the loop can provide: a
/// signal that arrives while the loop is blocked, from outside every event stream it is watching.
pub(crate) fn wake_after(
    waker: std::sync::Arc<dyn Waker>,
    delay: Duration,
    reason: impl FnOnce() -> zgui_platform::WakeReason + Send + 'static,
) {
    thread::spawn(move || {
        thread::sleep(delay);
        waker.wake(reason());
    });
}

/// Kills the program if the loop has not finished within the patience.
///
/// A parking failure that stalls produces a program that never returns, and a test suite that
/// never returns tells nobody anything. This turns that into a message and a non-zero exit.
pub(crate) fn watchdog(property: &'static str) {
    thread::spawn(move || {
        thread::sleep(PATIENCE);
        eprintln!(
            "FAILED: {property}: the loop never finished, which is the stall this asserts on"
        );
        std::process::exit(1);
    });
}

/// Announces that a property held.
pub(crate) fn passed(property: &str) {
    println!("ok: {property}");
}
