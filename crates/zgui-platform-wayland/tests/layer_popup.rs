//! A pop-up on a shell layer takes a grab from the press that opens it and is dismissed outside.
//!
//! Four properties, read off what the application is told:
//!
//! * **The press gives a serial.** The bar's handle reports the serial of the press while the
//!   press is being handled, and the pop-up opens with a grab on it.
//! * **The release does not dismiss.** The pop-up is still open well after the button that opened
//!   it came up.
//! * **A press outside dismisses.** A press on the empty desktop closes the pop-up with
//!   [`CloseReason::Dismissed`], and the bar stays open.
//! * **The bar draws on after the pop-up is gone.** The output then changes its mode, and the
//!   bar draws at the new width. That replaces the bar's swap chain after the pop-up's swap chain
//!   was destroyed, and the shutdown destroys the bar's swap chain last.
//!
//! A compositor without a virtual pointer has nothing to press with, and this program says it was
//! skipped. Only sway changes the mode of an output on request, so elsewhere the last property is
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
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use zgui_geom::{CssPx, Edges, Point, Rect, Size};
    use zgui_platform::{
        AppHandler, CloseReason, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId, WakeReason,
    };
    use zgui_runtime::{
        App, AppError, ExitPolicy, LayerOptions, PopupOptions, Runtime, WindowHandle, WindowOptions,
    };
    use zgui_view::prelude::GetUntracked;
    use zgui_view::{BuildCx, IntoView, View};
    use zgui_vocab::PointerAction;

    /// The height of the bar, in CSS pixels.
    const BAR: f32 = 60.0;

    /// The output mode after the dismissal, and the mode the run restores at the end.
    ///
    /// The bar spans the output, so its width follows the mode. The two widths fall in different
    /// size classes, so the renderer replaces the bar's swap chain.
    const MODES: [(u32, u32); 2] = [(1920, 1080), (1280, 720)];

    /// How many frames the bar draws at its new width before the program ends.
    const GROWN_FRAMES: u32 = 3;

    /// How long the pop-up has to stay open after the press that opened it.
    const STAYS: Duration = Duration::from_millis(400);

    /// How long the program waits for the dismissal.
    const PATIENCE: Duration = Duration::from_secs(6);

    /// What the program saw.
    #[derive(Clone, Copy, Debug, Default)]
    struct Seen {
        /// Whether a virtual pointer could be made.
        pointer: bool,
        /// Whether the press reported a serial.
        serial: bool,
        /// Whether the pop-up opened.
        opened: bool,
        /// Whether the pop-up was still open [`STAYS`] after it opened.
        stayed: bool,
        /// Why the pop-up closed.
        reason: Option<CloseReason>,
        /// Whether the bar was open at the end.
        bar_open: bool,
        /// Whether the compositor changed the output mode after the dismissal.
        mode_changed: bool,
        /// Whether the bar drew [`GROWN_FRAMES`] frames at its new width after the dismissal.
        grew: bool,
    }

    /// What the program saw, kept for after the loop.
    static SEEN: Mutex<Option<Seen>> = Mutex::new(None);

    /// Opens a menu from a press on the bar, and watches what becomes of it.
    struct Drive {
        /// The application.
        inner: Runtime,
        /// What the program has seen so far.
        seen: Seen,
        /// The bar.
        bar: Option<WindowHandle>,
        /// The menu, once the press opened it.
        menu: Option<WindowHandle>,
        /// When the menu was first seen open.
        opened_at: Option<Instant>,
        /// When the program started pressing.
        started: Option<Instant>,
        /// How many frames the bar drew at the width of the first of [`MODES`] after the dismissal.
        grown_frames: u32,
    }

    impl Drive {
        /// Keeps what was seen and finishes the loop.
        fn finish(&mut self, cx: &dyn PlatformCx) {
            if self.seen.mode_changed {
                set_mode(MODES[1]);
            }
            self.seen.bar_open = self.bar.as_ref().is_some_and(WindowHandle::is_open);
            *SEEN.lock().expect("the record is not poisoned") = Some(self.seen);
            cx.request_exit();
        }

        /// Starts the two clicks once the bar has drawn.
        fn start(&mut self, cx: &dyn PlatformCx) {
            if self.started.is_some() {
                return;
            }
            self.started = Some(Instant::now());
            // Fractions of the output: the first click is on the bar, the second on the empty
            // desktop below it.
            self.seen.pointer = support::virtual_pointer::clicks_over(
                (1000, 1000),
                vec![(500, 20), (500, 700)],
                Duration::from_millis(1200),
            );
            if !self.seen.pointer {
                self.finish(cx);
            }
        }

        /// Opens the menu against the bar, with the serial the press left behind.
        fn open_menu(&mut self) {
            let Some(bar) = self.bar.clone() else { return };
            let serial = bar.input_serial();
            self.seen.serial = serial.is_some();
            let anchor = Rect::new(
                Point::new(CssPx(300.0), CssPx(0.0)),
                Size::new(CssPx(40.0), CssPx(BAR)),
            );
            let menu = self.inner.scope().with(|| {
                zgui_runtime::use_windows().open(
                    WindowOptions::popup(
                        &bar,
                        PopupOptions::new(anchor)
                            .with_grab(serial)
                            .with_size(200.0, 150.0),
                    ),
                    || zgui_elements::column().class("root"),
                )
            });
            self.menu = Some(menu);
        }

        /// Looks at the menu once per frame.
        fn watch(&mut self, cx: &dyn PlatformCx) {
            if let Some(menu) = &self.menu {
                if menu.is_open() {
                    self.seen.opened = true;
                    let at = *self.opened_at.get_or_insert_with(Instant::now);
                    self.seen.stayed |= at.elapsed() >= STAYS;
                }
                if self.seen.reason.is_none()
                    && let Some(reason) = menu.close_reason().get_untracked()
                {
                    self.seen.reason = Some(reason);
                    self.seen.mode_changed = set_mode(MODES[0]);
                    if !self.seen.mode_changed {
                        self.finish(cx);
                        return;
                    }
                }
            }
            if self.grown_frames >= GROWN_FRAMES {
                self.seen.grew = true;
                self.finish(cx);
                return;
            }
            if self
                .started
                .is_some_and(|started| started.elapsed() > PATIENCE)
            {
                self.finish(cx);
            }
        }
    }

    impl AppHandler for Drive {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            self.inner.surfaces_available(cx);
            self.bar = self
                .inner
                .scope()
                .with(|| zgui_runtime::use_windows().all())
                .first()
                .cloned();
        }

        fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
            let drawn = matches!(event, SurfaceEvent::RedrawRequested);
            let pressed = matches!(
                event,
                SurfaceEvent::Pointer {
                    action: PointerAction::Pressed,
                    ..
                }
            );
            self.inner.surface_event(cx, surface, event);
            if cx.is_exiting() {
                return;
            }
            if pressed && self.menu.is_none() {
                self.open_menu();
            }
            if drawn {
                self.start(cx);
                // The pop-up is closed by then, so the frame is the bar's.
                let width = f64::from(MODES[0].0);
                let grown = cx.surface(surface).is_some_and(|bar| {
                    (f64::from(bar.size().width.0) / bar.scale_factor() - width).abs() < 0.5
                });
                if self.seen.reason.is_some() && grown {
                    self.grown_frames += 1;
                }
            }
            self.watch(cx);
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
        }

        fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
            self.inner.idle(cx)
        }

        fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
            self.inner.deadline_reached(cx);
            self.watch(cx);
        }

        fn shutting_down(&mut self, cx: &dyn PlatformCx) {
            self.inner.shutting_down(cx);
        }
    }

    /// Asks sway to set the mode of every output, and reports whether it did.
    fn set_mode((width, height): (u32, u32)) -> bool {
        std::env::var_os("SWAYSOCK").is_some()
            && support::ask_compositor(&format!("output * mode {width}x{height}"), "")
    }

    /// A bar across the top, which keeps the application running.
    fn application() -> Result<Runtime, AppError> {
        App::new()
            .with_application_id(support::APP_ID)
            .with_exit_policy(ExitPolicy::Explicit)
            .with_layer(
                LayerOptions::top()
                    .with_anchors(Edges::new(true, true, false, true))
                    .with_size(0.0, BAR)
                    .with_namespace("zgui-test-bar"),
            )
            .with_stylesheet("root { display: block; width: 100%; height: 100% }")
            .with_renderer(support::renderers())
            .into_handler(|cx: &mut BuildCx<'_>| {
                Box::new(zgui_elements::column().class("root").into_view().build(cx))
            })
    }

    pub(super) fn main() {
        const PROPERTY: &str = "a pop-up on a shell layer grabs from its press, survives the release, and is dismissed";

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
            bar: None,
            menu: None,
            opened_at: None,
            started: None,
            grown_frames: 0,
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
            .expect("the program reached its last step");
        if support::NO_DEVICE.load(Ordering::Relaxed) {
            support::skipped(
                PROPERTY,
                "this machine has no graphics device to draw through",
            );
            return;
        }
        if !seen.pointer {
            support::skipped(PROPERTY, "this compositor has no virtual pointer");
            return;
        }
        assert!(seen.serial, "the press left no serial: {seen:?}");
        assert!(seen.opened, "the pop-up never opened: {seen:?}");
        assert!(seen.stayed, "the release dismissed the pop-up: {seen:?}");
        assert_eq!(
            seen.reason,
            Some(CloseReason::Dismissed),
            "a press outside did not dismiss the pop-up: {seen:?}"
        );
        assert!(seen.bar_open, "the bar closed with its pop-up: {seen:?}");
        if !seen.mode_changed {
            support::skipped(
                "the bar draws on after its pop-up is gone",
                "only sway changes the mode of an output on request",
            );
            support::passed(
                PROPERTY,
                "opened with a grab, stayed through the release, dismissed by a press outside",
            );
            return;
        }
        assert!(seen.grew, "the bar did not draw at its new width: {seen:?}");
        support::passed(
            PROPERTY,
            "opened with a grab, stayed through the release, dismissed by a press outside, \
             and the bar drew on at a new width",
        );
    }
}
