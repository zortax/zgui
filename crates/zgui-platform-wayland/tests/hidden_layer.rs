//! A shell layer hidden before its first buffer draws once it is shown.
//!
//! A pop-over opens hidden and is shown on demand. Its surface is hidden before the shell has
//! configured it, so the shell still holds its first configure sequence when the show arrives.
//! The show must draw against that configure: an empty commit starts no new sequence on a surface
//! that never showed a buffer, and a show that waits for one waits for ever.

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

    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use zgui_geom::Edges;
    use zgui_platform::{
        AppHandler, ExclusiveZone, IdlePolicy, KeyboardInteractivity, PlatformCx, SurfaceEvent,
        SurfaceId, WakeReason,
    };
    use zgui_runtime::{App, AppError, LayerOptions, Runtime};
    use zgui_view::{BuildCx, IntoView, View};

    /// How many frames the layer drew after the show.
    static SHOWN_FRAMES: AtomicU64 = AtomicU64::new(0);

    /// How many frames make the property hold.
    const ENOUGH: u64 = 3;

    /// What the program is doing.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Step {
        /// The layer is about to open.
        Opening,
        /// The layer was hidden before its first buffer; the show is on its way.
        Hidden,
        /// The layer was shown; its frames are counted.
        Shown,
    }

    /// Drives the layer through the steps.
    struct Drive {
        /// The application.
        inner: Runtime,
        /// Where it is.
        step: Step,
        /// How a thread of this program reaches the loop.
        waker: Option<std::sync::Arc<dyn zgui_platform::Waker>>,
    }

    impl AppHandler for Drive {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            let waker = self.waker.get_or_insert_with(|| cx.waker()).clone();
            self.inner.surfaces_available(cx);
            if self.step != Step::Opening {
                return;
            }
            if let Some(layer) = cx.surfaces().first() {
                layer.set_visible(false);
                self.step = Step::Hidden;
                support::wake_after(waker.clone(), Duration::from_millis(300), || {
                    WakeReason::AppWork
                });
                // The second wake ends a run whose show never draws.
                support::wake_after(waker, Duration::from_secs(3), || WakeReason::AppWork);
            }
        }

        fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
            let drawn = matches!(event, SurfaceEvent::RedrawRequested);
            self.inner.surface_event(cx, surface, event);
            if !drawn || self.step != Step::Shown {
                return;
            }
            if SHOWN_FRAMES.fetch_add(1, Ordering::Relaxed) + 1 >= ENOUGH {
                cx.request_exit();
                return;
            }
            if let Some(layer) = cx.surface(surface) {
                layer.request_redraw();
            }
        }

        fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
            if matches!(reason, WakeReason::AppWork) {
                match self.step {
                    Step::Hidden => {
                        if let Some(layer) = cx.surfaces().first() {
                            layer.set_visible(true);
                        }
                        self.step = Step::Shown;
                    }
                    Step::Shown => cx.request_exit(),
                    Step::Opening => {}
                }
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

    /// A pop-over below the top edge that takes the keyboard on demand.
    fn popover() -> Result<Runtime, AppError> {
        App::new()
            .with_application_id(support::APP_ID)
            .with_layer(
                LayerOptions::top()
                    .with_anchors(Edges::new(true, false, false, true))
                    .with_size(200.0, 120.0)
                    .with_exclusive_zone(ExclusiveZone::Avoid)
                    .with_keyboard(KeyboardInteractivity::OnDemand)
                    .with_namespace("zgui-test-popover"),
            )
            .with_stylesheet("root { display: block; height: 120px }")
            .with_renderer(support::renderers())
            .into_handler(|cx: &mut BuildCx<'_>| {
                Box::new(zgui_elements::column().class("root").into_view().build(cx))
            })
    }

    pub(super) fn main() {
        const PROPERTY: &str = "a shell layer hidden before its first buffer draws once shown";

        if !support::require_nested(PROPERTY) {
            return;
        }

        support::tracing();
        support::watchdog(PROPERTY);

        let inner = match popover() {
            Ok(runtime) => runtime,
            Err(error) => {
                support::skipped(PROPERTY, &format!("the runtime would not install: {error}"));
                return;
            }
        };
        let handler = Drive {
            inner,
            step: Step::Opening,
            waker: None,
        };
        let Some(mut app) = support::loop_for(PROPERTY, handler) else {
            return;
        };
        if let Err(error) = app.run() {
            panic!("the loop stopped: {error}");
        }

        if support::NO_DEVICE.load(Ordering::Relaxed) {
            support::skipped(
                PROPERTY,
                "this machine has no graphics device to draw through",
            );
            return;
        }
        let shown = SHOWN_FRAMES.load(Ordering::Relaxed);
        assert!(
            shown >= ENOUGH,
            "the layer drew {shown} frames after the show; a show waits for a configure that never comes"
        );
        support::passed(
            PROPERTY,
            "the layer drew after a show that followed an early hide",
        );
    }
}
