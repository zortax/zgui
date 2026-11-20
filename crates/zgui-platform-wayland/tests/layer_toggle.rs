//! A shell layer shown and hidden faster than the shell answers keeps its connection.
//!
//! A pop-over opens and closes on every press. Each show patches its placement, and that commit
//! makes the shell configure the layer again. A hide that follows at once unmaps the layer while
//! the configure is on its way, so the configure arrives for a mapping that is gone. Acknowledged,
//! it is a protocol error on a wlroots shell, which ends the connection. The cycles here hide in
//! the delivery that commits the first buffer after a show, one turn after that buffer, after the
//! second buffer, and before any buffer, with a patch before every show and after every hide.
//! Every show that waits for a buffer must commit one, and the run must end the way the program
//! asks.

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

    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use zgui_geom::{CssPx, Edges};
    use zgui_platform::{
        AppHandler, ExclusiveZone, IdlePolicy, KeyboardInteractivity, LayerPatch, PlatformCx,
        Surface, SurfaceEvent, SurfaceId, WakeReason, Waker,
    };
    use zgui_runtime::{App, AppError, LayerOptions, Runtime};
    use zgui_view::{BuildCx, IntoView, View};

    /// How many show and hide cycles the program runs.
    const CYCLES: u64 = 200;

    /// How many cycles ended.
    static CYCLED: AtomicU64 = AtomicU64::new(0);

    /// How many shows ended in a hide with no buffer committed.
    static UNDRAWN: AtomicU64 = AtomicU64::new(0);

    /// When a cycle hides the layer.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Hide {
        /// In the delivery that commits the first buffer after the show.
        WithFirstBuffer,
        /// In the same turn as the show, before any buffer.
        Early,
        /// In the delivery that commits the second buffer after the show.
        WithSecondBuffer,
        /// One turn after the first buffer.
        NextTurn,
    }

    impl Hide {
        /// The way cycle `cycle` hides.
        const fn of(cycle: u64) -> Self {
            match cycle % 4 {
                0 => Self::WithFirstBuffer,
                1 => Self::Early,
                2 => Self::WithSecondBuffer,
                _ => Self::NextTurn,
            }
        }
    }

    /// Drives the layer through the cycles.
    struct Drive {
        /// The application.
        inner: Runtime,
        /// How this program reaches the loop on its next turn.
        waker: Option<Arc<dyn Waker>>,
        /// Whether the layer is hidden.
        hidden: bool,
        /// Whether a hide waits for the next turn.
        hide_next_turn: bool,
        /// How many buffers the program had committed when the layer was last shown.
        presents_at_show: u64,
    }

    impl Drive {
        /// The cycle now running.
        fn cycle() -> u64 {
            CYCLED.load(Ordering::Relaxed)
        }

        /// Asks for the next turn.
        fn next_turn(&self) {
            if let Some(waker) = &self.waker {
                waker.wake(WakeReason::AppWork);
            }
        }

        /// Changes the keyboard interactivity and the margin, which makes the shell configure again.
        fn patch(layer: &Arc<dyn Surface>, step: u64) {
            let keyboard = match step % 3 {
                0 => KeyboardInteractivity::Exclusive,
                1 => KeyboardInteractivity::None,
                _ => KeyboardInteractivity::OnDemand,
            };
            let left = CssPx((step % 7) as f32 * 12.0);
            let patch = LayerPatch::new()
                .with_keyboard(keyboard)
                .with_margin(Edges::new(CssPx(0.0), CssPx(0.0), CssPx(0.0), left));
            layer
                .set_layer_placement(&patch)
                .expect("a shell layer takes a placement patch");
        }

        /// Shows the layer for the next cycle, or ends the program after the last one.
        fn show(&mut self, cx: &dyn PlatformCx, layer: &Arc<dyn Surface>) {
            let cycle = Self::cycle();
            if cycle >= CYCLES {
                cx.request_exit();
                return;
            }
            Self::patch(layer, cycle * 2);
            layer.set_visible(true);
            self.hidden = false;
            self.presents_at_show = support::PRESENTS.load(Ordering::Relaxed);
            if Hide::of(cycle) == Hide::Early {
                self.hide(layer);
            }
        }

        /// Hides the layer, ends the cycle, and shows it again on the next turn.
        fn hide(&mut self, layer: &Arc<dyn Surface>) {
            if support::PRESENTS.load(Ordering::Relaxed) == self.presents_at_show {
                UNDRAWN.fetch_add(1, Ordering::Relaxed);
            }
            layer.set_visible(false);
            self.hidden = true;
            self.hide_next_turn = false;
            let cycle = CYCLED.fetch_add(1, Ordering::Relaxed);
            Self::patch(layer, cycle * 2 + 1);
            self.next_turn();
        }
    }

    impl AppHandler for Drive {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            self.waker.get_or_insert_with(|| cx.waker());
            self.inner.surfaces_available(cx);
        }

        fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
            let delivery = matches!(
                event,
                SurfaceEvent::RedrawRequested | SurfaceEvent::PresentReady
            );
            self.inner.surface_event(cx, surface, event);
            if !delivery || self.hidden || self.hide_next_turn {
                return;
            }
            let Some(layer) = cx.surface(surface) else {
                return;
            };
            let presented = support::PRESENTS.load(Ordering::Relaxed) - self.presents_at_show;
            match Hide::of(Self::cycle()) {
                Hide::WithFirstBuffer if presented >= 1 => self.hide(&layer),
                Hide::WithSecondBuffer if presented >= 2 => self.hide(&layer),
                Hide::NextTurn if presented >= 1 => {
                    self.hide_next_turn = true;
                    self.next_turn();
                }
                _ => layer.request_redraw(),
            }
        }

        fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
            if matches!(reason, WakeReason::AppWork)
                && let Some(layer) = cx.surfaces().first().cloned()
            {
                if self.hide_next_turn {
                    self.hide(&layer);
                } else if self.hidden {
                    self.show(cx, &layer);
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

    /// A pop-over below the top edge whose content changes every frame.
    fn popover() -> Result<Runtime, AppError> {
        App::new()
            .with_application_id(support::APP_ID)
            .with_layer(
                LayerOptions::top()
                    .with_anchors(Edges::new(true, false, false, true))
                    .with_size(200.0, 120.0)
                    .with_exclusive_zone(ExclusiveZone::Avoid)
                    .with_keyboard(KeyboardInteractivity::OnDemand)
                    .with_namespace("zgui-test-toggle"),
            )
            .with_stylesheet(
                "@keyframes pulse { from { opacity: 1 } to { opacity: 0.4 } }
                 root { display: block; height: 120px; background: #345;
                        animation: pulse 300ms linear infinite }",
            )
            .with_renderer(support::renderers())
            .into_handler(|cx: &mut BuildCx<'_>| {
                Box::new(zgui_elements::column().class("root").into_view().build(cx))
            })
    }

    pub(super) fn main() {
        const PROPERTY: &str =
            "a shell layer toggled faster than the shell answers keeps its connection";

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
            waker: None,
            hidden: false,
            hide_next_turn: false,
            presents_at_show: 0,
        };
        let Some(mut app) = support::loop_for(PROPERTY, handler) else {
            return;
        };
        let started = std::time::Instant::now();
        if let Err(error) = app.run() {
            panic!(
                "FAILED: {PROPERTY}: the loop stopped after {} cycles: {error}",
                CYCLED.load(Ordering::Relaxed)
            );
        }

        if support::NO_DEVICE.load(Ordering::Relaxed) {
            support::skipped(
                PROPERTY,
                "this machine has no graphics device to draw through",
            );
            return;
        }
        let cycled = CYCLED.load(Ordering::Relaxed);
        assert_eq!(
            cycled, CYCLES,
            "FAILED: {PROPERTY}: the run ended after {cycled} cycles"
        );
        // Only the shows that hide before any buffer end without one.
        let undrawn = UNDRAWN.load(Ordering::Relaxed);
        assert_eq!(
            undrawn,
            CYCLES / 4,
            "FAILED: {PROPERTY}: {undrawn} shows ended without a buffer"
        );
        let presents = support::PRESENTS.load(Ordering::Relaxed);
        support::passed(
            PROPERTY,
            &format!(
                "{cycled} cycles in {:?}, {presents} buffers committed",
                started.elapsed()
            ),
        );
    }
}
