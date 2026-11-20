//! A focus grab is offered only where the compositor has the global, and its requests are accepted.
//!
//! Two properties:
//!
//! * **The capability is the global.** A compositor without `hyprland_focus_grab_manager_v1`
//!   declares no focus grab. A grab made there stays inactive.
//! * **A grab's requests are accepted.** Where the global exists, a grab over a pop-over starts,
//!   takes a second window, lets go of it, and is destroyed, with no protocol error. A second grab
//!   then starts over the bar. A compositor may clear the first grab when that happens, and the
//!   program reports whether it did.
//!
//! A press outside the whitelist clears a grab, but a virtual pointer refreshes the pointer focus
//! only after a relative motion on Hyprland, so this program does not press.

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

    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Mutex;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};

    use zgui_geom::Edges;
    use zgui_platform::{AppHandler, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId, WakeReason};
    use zgui_runtime::grab::{ClearedGuard, FocusGrab};
    use zgui_runtime::{
        App, AppError, ExitPolicy, LayerOptions, Runtime, WindowHandle, WindowOptions,
    };
    use zgui_view::prelude::GetUntracked;
    use zgui_view::{BuildCx, IntoView, View};

    /// What the program saw: whether the grab was offered, whether it became active, and whether the
    /// second grab cleared the first.
    static SEEN: Mutex<Option<(bool, bool, bool)>> = Mutex::new(None);

    /// How long the program waits for the compositor to clear the first grab.
    const PATIENCE: Duration = Duration::from_millis(800);

    /// Drives a grab through its requests.
    struct Drive {
        /// The application.
        inner: Runtime,
        /// Whether the desktop says it can grab.
        offered: bool,
        /// The pop-overs, once they are asked for.
        popovers: Vec<WindowHandle>,
        /// The grab over the first pop-over.
        first: Option<FocusGrab>,
        /// The grab over the bar.
        second: Option<FocusGrab>,
        /// What keeps the clear callback of the first grab.
        guard: Option<ClearedGuard>,
        /// Whether the first grab was cleared.
        cleared: Rc<Cell<bool>>,
        /// Whether the first grab became active.
        activated: bool,
        /// How many frames have been drawn since the grab was made.
        frames: u32,
        /// When the second grab was made.
        since: Option<Instant>,
    }

    impl Drive {
        /// Finishes the loop and keeps what was seen.
        fn finish(&mut self, cx: &dyn PlatformCx) {
            self.guard = None;
            self.first = None;
            self.second = None;
            *SEEN.lock().expect("the record is not poisoned") =
                Some((self.offered, self.activated, self.cleared.get()));
            cx.request_exit();
        }

        /// Takes the grab one step further, once per frame.
        fn step(&mut self, cx: &dyn PlatformCx) {
            let scope = self.inner.scope();
            if self.first.is_none() {
                let [one, two] = &self.popovers[..] else {
                    return;
                };
                if !one.is_open() || !two.is_open() {
                    return;
                }
                let grab = scope.with(|| FocusGrab::new(&[one]));
                let cleared = Rc::clone(&self.cleared);
                self.guard = grab
                    .as_ref()
                    .map(|grab| grab.on_cleared(move || cleared.set(true)));
                self.first = grab;
                return;
            }
            self.frames += 1;
            let (Some(first), [one, two]) = (&self.first, &self.popovers[..]) else {
                return;
            };
            self.activated |= first.active().get_untracked();
            match self.frames {
                2 => first.add(two),
                4 => first.remove(one),
                6 => {
                    let bar = scope.with(|| zgui_runtime::use_windows().all())[0].clone();
                    self.second = scope.with(|| FocusGrab::new(&[&bar]));
                    self.since = Some(Instant::now());
                }
                _ => {}
            }
            if self
                .since
                .is_some_and(|since| self.cleared.get() || since.elapsed() > PATIENCE)
            {
                self.finish(cx);
            }
        }
    }

    impl AppHandler for Drive {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            self.inner.surfaces_available(cx);
            self.offered = cx.capabilities().focus_grab;
            if !self.offered {
                let scope = self.inner.scope();
                let bar = scope.with(|| zgui_runtime::use_windows().all())[0].clone();
                self.first = scope.with(|| FocusGrab::new(&[&bar]));
                assert!(self.first.is_some(), "a running application made no grab");
                return;
            }
            let open = |inner: &Runtime, namespace: &'static str| {
                inner.scope().with(|| {
                    zgui_runtime::use_windows().open(
                        WindowOptions::layer(
                            LayerOptions::top()
                                .with_anchors(Edges::new(true, false, false, true))
                                .with_margin(40.0, 0.0, 0.0, 8.0)
                                .with_size(200.0, 120.0)
                                .with_namespace(namespace),
                        ),
                        || zgui_elements::column().class("root"),
                    )
                })
            };
            self.popovers = vec![
                open(&self.inner, "zgui-test-popover-one"),
                open(&self.inner, "zgui-test-popover-two"),
            ];
        }

        fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
            let drawn = matches!(event, SurfaceEvent::RedrawRequested);
            self.inner.surface_event(cx, surface, event);
            if !drawn || cx.is_exiting() {
                return;
            }
            if self.offered {
                self.step(cx);
            } else {
                // A few frames give the runtime the turns it starts a grab on.
                self.frames += 1;
                self.activated |= self
                    .first
                    .as_ref()
                    .is_some_and(|grab| grab.active().get_untracked());
                if self.frames >= 4 {
                    self.finish(cx);
                }
            }
            // A still document asks for one frame, so each frame asks for the next.
            if !cx.is_exiting()
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
        }

        fn shutting_down(&mut self, cx: &dyn PlatformCx) {
            self.inner.shutting_down(cx);
        }
    }

    /// A bar across the top, which keeps the application running.
    fn application() -> Result<Runtime, AppError> {
        App::new()
            .with_application_id(support::APP_ID)
            .with_exit_policy(ExitPolicy::Explicit)
            .with_layer(
                LayerOptions::top()
                    .with_anchors(Edges::new(true, true, false, true))
                    .with_size(0.0, 30.0)
                    .with_namespace("zgui-test-bar"),
            )
            .with_stylesheet("root { display: block; width: 100%; height: 100% }")
            .with_renderer(support::renderers())
            .into_handler(|cx: &mut BuildCx<'_>| {
                Box::new(zgui_elements::column().class("root").into_view().build(cx))
            })
    }

    pub(super) fn main() {
        const PROPERTY: &str =
            "a focus grab is offered only with its global, and its requests are accepted";

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
            offered: false,
            popovers: Vec::new(),
            first: None,
            second: None,
            guard: None,
            cleared: Rc::default(),
            activated: false,
            frames: 0,
            since: None,
        };
        let Some(mut app) = support::loop_for(PROPERTY, handler) else {
            return;
        };
        if let Err(error) = app.run() {
            panic!("the loop stopped: {error}");
        }
        let (offered, activated, cleared) = SEEN
            .lock()
            .expect("the record is not poisoned")
            .expect("the program reached its last step");
        if !offered {
            assert!(!activated, "a grab became active without the global");
            support::passed(PROPERTY, "no global, so no capability and an inactive grab");
            return;
        }
        if support::NO_DEVICE.load(Ordering::Relaxed) {
            support::skipped(
                PROPERTY,
                "this machine has no graphics device to draw through",
            );
            return;
        }
        assert!(activated, "the grab never became active");
        support::passed(
            PROPERTY,
            if cleared {
                "the grab started, changed and stopped; the second grab cleared it"
            } else {
                "the grab started, changed and stopped; the second grab left it as it was"
            },
        );
    }
}
