//! The lock keys reach the window that holds the keyboard.
//!
//! One property, read off the window handle: **a lock the compositor reports is the lock the
//! window reads.** A virtual keyboard locks caps lock, then num lock alone, then nothing, and the
//! window's `caps_lock` and `lock_keys` follow each step.
//!
//! A compositor without a virtual keyboard has nothing to lock with, and this program says it was
//! skipped.

#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
#[path = "support/mod.rs"]
mod support;

/// Runs the test where the platform under test exists, and does nothing elsewhere.
fn main() {
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    supported::main();
}

/// The test, built only where the platform under test exists.
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
mod supported {
    use super::support;

    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use zgui_platform::{AppHandler, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId, WakeReason};
    use zgui_runtime::{Runtime, WindowHandle};
    use zgui_view::prelude::GetUntracked;
    use zgui_vocab::Modifiers;

    /// How long the program waits for the compositor.
    const PATIENCE: Duration = Duration::from_secs(8);

    /// The lock sets the window has to read, in order.
    const EXPECTED: [Modifiers; 3] = [Modifiers::CAPS_LOCK, Modifiers::NUM_LOCK, Modifiers::NONE];

    /// What the program saw.
    #[derive(Clone, Debug, Default)]
    struct Seen {
        /// Whether a virtual keyboard could be made.
        keyboard: bool,
        /// Each distinct lock set the window read, in order.
        locks: Vec<Modifiers>,
        /// Whether caps lock read as on while the caps lock step held.
        caps: bool,
    }

    /// What the program saw, kept for after the loop.
    static SEEN: Mutex<Option<Seen>> = Mutex::new(None);

    /// Waits for the keyboard, then follows the locks the window reads.
    struct Drive {
        /// The application.
        inner: Runtime,
        /// What the program has seen so far.
        seen: Seen,
        /// The window.
        window: Option<WindowHandle>,
        /// When the virtual keyboard was started.
        since: Option<Instant>,
    }

    impl Drive {
        /// Keeps what was seen and finishes the loop.
        fn finish(&mut self, cx: &dyn PlatformCx) {
            *SEEN.lock().expect("the record is not poisoned") = Some(self.seen.clone());
            cx.request_exit();
        }

        /// Starts the keyboard once the window holds the keyboard, and reads the locks.
        fn step(&mut self, cx: &dyn PlatformCx) {
            if cx.is_exiting() {
                return;
            }
            let Some(window) = self.window.clone() else {
                return;
            };
            if self.since.is_none() {
                if !window.focused().get_untracked() {
                    return;
                }
                self.since = Some(Instant::now());
                self.seen.keyboard = support::virtual_keyboard::lock_over(
                    vec![
                        support::virtual_keyboard::CAPS_LOCK,
                        support::virtual_keyboard::NUM_LOCK,
                        0,
                    ],
                    Duration::from_millis(400),
                );
                if !self.seen.keyboard {
                    self.finish(cx);
                }
                return;
            }
            let locks = window.lock_keys().get_untracked();
            if self.seen.locks.last() != Some(&locks)
                && (locks != Modifiers::NONE || !self.seen.locks.is_empty())
            {
                self.seen.locks.push(locks);
                if locks == Modifiers::CAPS_LOCK {
                    self.seen.caps = window.caps_lock().get_untracked();
                }
            }
            if self.seen.locks.len() >= EXPECTED.len()
                || self.since.is_some_and(|since| since.elapsed() > PATIENCE)
            {
                self.finish(cx);
            }
        }
    }

    impl AppHandler for Drive {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            self.inner.surfaces_available(cx);
            self.window = self
                .inner
                .windows()
                .first()
                .map(|window| window.handle().clone());
        }

        fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
            let drawn = matches!(event, SurfaceEvent::RedrawRequested);
            self.inner.surface_event(cx, surface, event);
            self.step(cx);
            // A still document asks for one frame, so each frame asks for the next.
            if drawn
                && !cx.is_exiting()
                && let Some(held) = cx.surface(surface)
            {
                held.request_redraw();
            }
        }

        fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
            self.inner.wake(cx, reason);
            self.step(cx);
        }

        fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
            self.inner.idle(cx)
        }

        fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
            self.inner.deadline_reached(cx);
            self.step(cx);
        }

        fn shutting_down(&mut self, cx: &dyn PlatformCx) {
            self.inner.shutting_down(cx);
        }
    }

    pub(super) fn main() {
        const PROPERTY: &str = "the lock keys reach the window that holds the keyboard";

        if !support::require_nested(PROPERTY) {
            return;
        }

        support::tracing();
        support::watchdog(PROPERTY);

        let inner = match support::application("zgui lock keys") {
            Ok(runtime) => runtime,
            Err(error) => {
                support::skipped(PROPERTY, &format!("the runtime would not install: {error}"));
                return;
            }
        };
        let handler = Drive {
            inner,
            seen: Seen::default(),
            window: None,
            since: None,
        };
        let Some(mut app) = support::loop_for(PROPERTY, handler) else {
            return;
        };
        if let Err(error) = app.run() {
            panic!("the loop stopped: {error}");
        }
        let seen = SEEN
            .lock()
            .expect("the record is not poisoned")
            .clone()
            .expect("the program reached its last step");
        if !seen.keyboard {
            support::skipped(PROPERTY, "this compositor has no virtual keyboard");
            return;
        }
        assert_eq!(
            seen.locks, EXPECTED,
            "the window read other locks: {seen:?}"
        );
        assert!(seen.caps, "caps lock did not read as on: {seen:?}");
        support::passed(PROPERTY, "caps lock, then num lock, then none");
    }
}
