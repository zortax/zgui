//! The session lock is offered only with its global, confirmed with lock windows, and unlocked.
//!
//! Six properties, read off what the application is told:
//!
//! * **The capability is the global.** A compositor without `ext_session_lock_manager_v1`
//!   declares no session lock, and a lock asked for there ends at once.
//! * **A lock window covers its output.** The compositor configures each lock window at the
//!   extent of its output, and confirms the lock once every output shows one.
//! * **An output takes one lock window.** A second lock surface on an output is refused before
//!   the compositor sees it, because the compositor answers it with a protocol error.
//! * **A lock window follows its output.** The output changes its mode while the session is
//!   locked. The lock window draws at the new extent, and the compositor raises no protocol
//!   error. Only sway changes a mode on request, so elsewhere this property is skipped.
//! * **A lock window reports its first frame on the screen.** The compositor shows a frame of each
//!   lock window, and each window says so before the unlock.
//! * **An unlock closes the lock windows.** The windows close with
//!   [`CloseReason::Application`], and the compositor raises no protocol error.
//!
//! This program locks the session of the compositor it runs on, so it runs only on a nested
//! compositor: `cargo xtask wayland` starts one.

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
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use zgui_geom::{Css, CssPx, Size};
    use zgui_platform::{AppHandler, CloseReason, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId};
    use zgui_platform::{MonitorInfo, SurfaceAttributes, WakeReason};
    use zgui_runtime::lock::{LockHandle, SessionLock};
    use zgui_runtime::{App, AppError, ExitPolicy, Runtime};
    use zgui_view::prelude::GetUntracked;

    /// How long the program waits for the compositor.
    const PATIENCE: Duration = Duration::from_secs(8);

    /// The output mode set while the session is locked, and the mode the run restores at the end.
    const MODES: [(u32, u32); 2] = [(1600, 900), (1280, 720)];

    /// How many frames the lock window draws at the new extent before the unlock.
    const FOLLOWED_FRAMES: u32 = 3;

    /// Where the program is.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    enum Step {
        /// Waiting for the confirmation and the first frames.
        #[default]
        Locking,
        /// The output mode changed; waiting for frames at the new extent.
        Following,
        /// The unlock was asked for.
        Unlocking,
    }

    /// What the program saw.
    #[derive(Clone, Debug, Default)]
    struct Seen {
        /// Whether the compositor offers a session lock.
        offered: bool,
        /// Whether the lock ended at once on a compositor that offers none.
        ended_at_once: bool,
        /// The outputs, as the application read them.
        outputs: Vec<MonitorInfo>,
        /// Whether the compositor confirmed the lock.
        locked: bool,
        /// The extents of the lock windows when the lock was confirmed.
        extents: Vec<Size<CssPx, Css>>,
        /// Whether a second lock surface on an output was refused.
        duplicate_refused: bool,
        /// Whether the compositor changed the output mode while the session was locked.
        mode_changed: bool,
        /// Whether the lock window drew [`FOLLOWED_FRAMES`] frames at the new extent.
        followed: bool,
        /// Whether every lock window reported a frame on the screen.
        presented: bool,
        /// Why each lock window closed.
        reasons: Vec<Option<CloseReason>>,
    }

    /// What the program saw, kept for after the loop.
    static SEEN: Mutex<Option<Seen>> = Mutex::new(None);

    /// Locks the session, waits for the confirmation, and unlocks.
    struct Drive {
        /// The application.
        inner: Runtime,
        /// What the program has seen so far.
        seen: Seen,
        /// The lock, once it is asked for.
        lock: Option<LockHandle>,
        /// Where the program is.
        step: Step,
        /// How many frames were drawn at the new extent.
        followed_frames: u32,
        /// When the lock was asked for.
        since: Option<Instant>,
    }

    impl Drive {
        /// Keeps what was seen and finishes the loop.
        fn finish(&mut self, cx: &dyn PlatformCx) {
            if self.seen.mode_changed {
                set_mode(MODES[1]);
            }
            *SEEN.lock().expect("the record is not poisoned") = Some(self.seen.clone());
            cx.request_exit();
        }

        /// Takes the lock one step further.
        fn step(&mut self, cx: &dyn PlatformCx) {
            let Some(lock) = self.lock.clone() else {
                return;
            };
            if cx.is_exiting() {
                return;
            }
            if !self.seen.offered {
                self.seen.ended_at_once = lock.finished().get_untracked();
                self.finish(cx);
                return;
            }
            if self.step == Step::Locking && lock.locked().get_untracked() {
                // A frame at the configured extent has to reach the screen before the extent is read.
                let windows = lock.windows();
                if windows
                    .iter()
                    .all(|window| window.size().get_untracked().width.0 > 1.0)
                {
                    self.seen.locked = true;
                    self.seen.extents = windows
                        .iter()
                        .map(|window| window.size().get_untracked())
                        .collect();
                    self.seen.duplicate_refused = self.duplicate_refused(cx);
                    self.seen.mode_changed = set_mode(MODES[0]);
                    self.step = Step::Following;
                }
            }
            let presented = lock
                .windows()
                .iter()
                .all(|window| window.presented().get_untracked());
            if self.step == Step::Following
                && presented
                && (!self.seen.mode_changed || self.followed_frames >= FOLLOWED_FRAMES)
            {
                self.seen.followed = self.followed_frames >= FOLLOWED_FRAMES;
                self.seen.presented = true;
                if lock.unlock() {
                    self.step = Step::Unlocking;
                }
            }
            let windows = lock.windows();
            let closed = windows.iter().all(|window| !window.is_open());
            if (self.step == Step::Unlocking && closed) || lock.finished().get_untracked() {
                self.seen.reasons = windows
                    .iter()
                    .map(|window| window.close_reason().get_untracked())
                    .collect();
                self.finish(cx);
                return;
            }
            if self.since.is_some_and(|since| since.elapsed() > PATIENCE) {
                self.finish(cx);
            }
        }
    }

    impl Drive {
        /// Asks for a second lock surface on the first output, and reports whether it was refused.
        fn duplicate_refused(&self, cx: &dyn PlatformCx) -> bool {
            let Some(id) = self.seen.outputs.iter().find_map(|monitor| monitor.id) else {
                return false;
            };
            match cx.create_surface(&SurfaceAttributes::lock(id)) {
                Ok(surface) => {
                    cx.destroy_surface(surface.id());
                    false
                }
                Err(_) => true,
            }
        }

        /// Counts a frame of `surface` when it is drawn at the extent of the first of [`MODES`].
        fn count_followed(&mut self, cx: &dyn PlatformCx, surface: SurfaceId) {
            if self.step != Step::Following || !self.seen.mode_changed {
                return;
            }
            let (width, height) = MODES[0];
            let at_new_extent = cx.surface(surface).is_some_and(|held| {
                let scale = held.scale_factor();
                let size = held.size();
                (f64::from(size.width.0) / scale - f64::from(width)).abs() < 0.5
                    && (f64::from(size.height.0) / scale - f64::from(height)).abs() < 0.5
            });
            if at_new_extent {
                self.followed_frames += 1;
            }
        }
    }

    /// Asks sway to set the mode of every output, and reports whether it did.
    fn set_mode((width, height): (u32, u32)) -> bool {
        std::env::var_os("SWAYSOCK").is_some()
            && support::ask_compositor(&format!("output * mode {width}x{height}"), "")
    }

    impl AppHandler for Drive {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            self.inner.surfaces_available(cx);
            self.seen.offered = cx.capabilities().session_lock;
            let scope = self.inner.scope();
            let lock = scope.with(SessionLock::lock).expect("no lock is held yet");
            self.seen.outputs = scope.with(|| zgui_runtime::use_monitors().get_untracked());
            for id in self.seen.outputs.iter().filter_map(|monitor| monitor.id) {
                scope.with(|| {
                    lock.open_surface(id, || zgui_elements::column().class("root"));
                });
            }
            self.lock = Some(lock);
            self.since = Some(Instant::now());
        }

        fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
            let drawn = matches!(event, SurfaceEvent::RedrawRequested);
            self.inner.surface_event(cx, surface, event);
            if drawn {
                self.count_followed(cx, surface);
            }
            self.step(cx);
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

    /// An application with no window of its own.
    fn application() -> Result<Runtime, AppError> {
        App::new()
            .with_application_id(support::APP_ID)
            .without_primary_window()
            .with_exit_policy(ExitPolicy::Explicit)
            .with_stylesheet("root { display: block; width: 100%; height: 100% }")
            .with_renderer(support::renderers())
            .into_handler(|_cx: &mut zgui_view::BuildCx<'_>| {
                unreachable!("there is no primary window")
            })
    }

    pub(super) fn main() {
        const PROPERTY: &str =
            "a session lock is offered with its global, covers each output, and unlocks";

        if !support::require_nested(PROPERTY) {
            return;
        }

        support::tracing();
        support::watchdog(PROPERTY);

        let inner = match application() {
            Ok(runtime) => runtime,
            Err(error) => {
                support::skipped(PROPERTY, &format!("the runtime would not install: {error}"));
                return;
            }
        };
        let handler = Drive {
            inner,
            seen: Seen::default(),
            lock: None,
            step: Step::Locking,
            followed_frames: 0,
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
        if !seen.offered {
            assert!(seen.ended_at_once, "a lock without the global did not end");
            support::passed(PROPERTY, "no global, so no capability, and the lock ended");
            return;
        }
        if support::NO_DEVICE.load(Ordering::Relaxed) {
            support::skipped(
                PROPERTY,
                "this machine has no graphics device to draw through",
            );
            return;
        }
        assert!(
            seen.locked,
            "the compositor never confirmed the lock: {seen:?}"
        );
        for (extent, output) in seen.extents.iter().zip(&seen.outputs) {
            let logical = output
                .logical
                .expect("the compositor states a logical extent")
                .size;
            assert_eq!(
                (extent.width.0.round(), extent.height.0.round()),
                (logical.width.0.round(), logical.height.0.round()),
                "a lock window does not cover its output: {seen:?}"
            );
        }
        assert!(
            seen.duplicate_refused,
            "a second lock surface on an output was asked for: {seen:?}"
        );
        assert!(
            seen.presented,
            "a lock window never reported a frame on the screen: {seen:?}"
        );
        if seen.mode_changed {
            assert!(
                seen.followed,
                "the lock window did not draw at the new extent: {seen:?}"
            );
        } else {
            support::skipped(
                "a lock window follows its output",
                "only sway changes the mode of an output on request",
            );
        }
        assert!(
            seen.reasons
                .iter()
                .all(|reason| *reason == Some(CloseReason::Application)),
            "the lock windows did not close with the unlock: {seen:?}"
        );
        support::passed(
            PROPERTY,
            &format!(
                "{} lock window(s) at the extent of their outputs, a second one refused, {}closed \
                 by the unlock",
                seen.extents.len(),
                if seen.mode_changed {
                    "drawn at the new mode, "
                } else {
                    ""
                }
            ),
        );
    }
}
