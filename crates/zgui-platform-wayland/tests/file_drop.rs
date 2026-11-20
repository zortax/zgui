//! A file dragged from another application and dropped on a window arrives as a drop.
//!
//! The compositor completes a drop only when the destination accepted a media type and an action
//! was chosen for it. Without either one, the compositor cancels the drop when the button comes up,
//! and nothing reaches the application. So the drag here comes from a real source on a connection
//! of its own, and the assertions are made on both ends: the application saw the paths dropped,
//! and the source was told the drop finished with a copy.
//!
//! A drag of text goes first. The window takes only files, so it must not choose an action for
//! that drag: a chosen action makes the source show a copy cursor over a window that then cancels
//! the drop.

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

    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use wayland_client::protocol::wl_data_device_manager::DndAction;
    use zgui_platform::{
        AppHandler, DragEvent, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId, WakeReason,
    };

    /// The text a drag with no files on it carries.
    const TEXT: &str = "a few dragged words";

    /// The file list the source offers.
    const PAYLOAD: &str = "file:///tmp/zgui%20dropped\r\n";

    /// The path that list names.
    const DROPPED: &str = "/tmp/zgui dropped";

    /// Where the drag is let go, in thousandths of the output, on the destination's half.
    const TO: (u32, u32) = (250, 500);

    /// Whether the drag was reported as having entered the window.
    static ENTERED: AtomicBool = AtomicBool::new(false);

    /// The paths the window was told were dropped on it.
    static DROPPED_PATHS: Mutex<Option<Vec<PathBuf>>> = Mutex::new(None);

    /// What the source was told about each drag, or why no drag could start.
    static OUTCOMES: Mutex<Option<Result<Outcomes, String>>> = Mutex::new(None);

    /// What the source was told about the text drag and the file drag.
    type Outcomes = (support::drag_source::Outcome, support::drag_source::Outcome);

    /// Starts the drag once the window is drawn, and ends the loop when the source is done.
    struct DropOnWindow<A: AppHandler> {
        /// The application.
        inner: A,
        /// Whether the drag has been started.
        started: bool,
        /// How the source thread ends this loop.
        waker: Option<std::sync::Arc<dyn zgui_platform::Waker>>,
    }

    impl<A: AppHandler> AppHandler for DropOnWindow<A> {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            self.waker.get_or_insert_with(|| cx.waker());
            self.inner.surfaces_available(cx);
        }

        fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
            match &event {
                SurfaceEvent::Drag(DragEvent::Entered { .. }) => {
                    ENTERED.store(true, Ordering::Relaxed);
                }
                SurfaceEvent::Drag(DragEvent::Dropped { paths, .. }) => {
                    *DROPPED_PATHS.lock().expect("the record is not poisoned") =
                        Some(paths.clone());
                }
                _ => {}
            }
            let drawn = matches!(event, SurfaceEvent::RedrawRequested);
            self.inner.surface_event(cx, surface, event);
            if !drawn || self.started {
                return;
            }
            self.started = true;
            let Some(waker) = self.waker.clone() else {
                return;
            };
            // On a thread of its own, because the source waits for the compositor at every step and
            // this loop has to keep answering the drag while it does.
            std::thread::spawn(move || {
                let outcomes = support::drag_source::drag(TO, "text/plain;charset=utf-8", TEXT)
                    .and_then(|text| {
                        support::drag_source::drag(TO, "text/uri-list", PAYLOAD)
                            .map(|files| (text, files))
                    });
                *OUTCOMES.lock().expect("the record is not poisoned") = Some(outcomes);
                zgui_platform::Waker::wake(waker.as_ref(), WakeReason::DeviceLost);
            });
        }

        fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
            if matches!(reason, WakeReason::DeviceLost) {
                cx.request_exit();
            }
            self.inner.wake(cx, reason);
        }

        fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
            self.inner.idle(cx)
        }

        fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
            self.inner.deadline_reached(cx);
        }

        fn shutting_down(&mut self, cx: &dyn PlatformCx) {
            self.inner.shutting_down(cx);
        }
    }

    pub(super) fn main() {
        const PROPERTY: &str = "a file dragged from another application is dropped on the window";

        if !support::require_nested(PROPERTY) {
            return;
        }

        support::tracing();
        support::watchdog(PROPERTY);

        let handler = DropOnWindow {
            inner: match support::application("file drop") {
                Ok(runtime) => runtime,
                Err(error) => {
                    support::skipped(PROPERTY, &format!("the runtime would not install: {error}"));
                    return;
                }
            },
            started: false,
            waker: None,
        };

        let Some(mut app) = support::loop_for(PROPERTY, handler) else {
            return;
        };
        app.run().expect("the loop ran");

        if support::NO_DEVICE.load(Ordering::Relaxed) {
            support::skipped(
                PROPERTY,
                "this machine has no graphics device to draw through",
            );
            return;
        }
        let (text, outcome) = match OUTCOMES.lock().expect("the record is not poisoned").take() {
            Some(Ok(outcomes)) => outcomes,
            Some(Err(reason)) => {
                support::skipped(PROPERTY, &format!("no drag could start here: {reason}"));
                return;
            }
            None => panic!("the source never reported"),
        };

        assert!(
            !text.claimed && text.cancelled,
            "the window chose an action for a drag with no files on it: {text:?}"
        );
        assert!(
            ENTERED.load(Ordering::Relaxed),
            "the drag never entered the window: {outcome:?}"
        );
        assert!(
            !outcome.cancelled,
            "the compositor cancelled the drop: {outcome:?}"
        );
        assert!(
            outcome.performed && outcome.finished,
            "the drop was not performed and finished: {outcome:?}"
        );
        assert_eq!(
            outcome.action,
            Some(DndAction::Copy),
            "the drop was not a copy: {outcome:?}"
        );
        let paths = DROPPED_PATHS
            .lock()
            .expect("the record is not poisoned")
            .clone();
        assert_eq!(
            paths,
            Some(vec![PathBuf::from(DROPPED)]),
            "the window was not told what was dropped on it"
        );
        support::passed(
            PROPERTY,
            "text left unclaimed; files entered, dropped, finished as a copy",
        );
    }
}
