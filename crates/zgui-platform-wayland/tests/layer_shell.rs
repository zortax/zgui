//! A shell layer keeps its placement through a change and through a hide and a show.
//!
//! Each property is read off the compositor or off the events the application hears:
//!
//! * **The reserved room is the room asked for.** A bar that reserves 34 pixels at the top moves
//!   the space windows get down by 34 pixels.
//! * **A placement changed at run time reaches the compositor.** The change goes out with the next
//!   frame, so the reserved room grows once a frame has been drawn.
//! * **A requested extent reaches the application.** The bar asks for the new height with the
//!   change, and the application hears a resize to it.
//! * **A placement and the regions change in one frame.** The input and opaque regions go out with
//!   the same commit, rounded to whole pixels, and the compositor accepts them.
//! * **A remap sends the placement again.** The shell forgets everything about a layer surface
//!   that is unmapped. A remap that only committed would leave a surface with no anchors and no
//!   size, which is a protocol error for a bar and ends the connection.
//!
//! Only sway answers the question of how much room is reserved, through the area its workspace
//! is given. On another compositor this program says it was skipped.

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
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use zgui_geom::{CssPx, Edges, Point, Rect, Size};
    use zgui_platform::{
        AppHandler, ExclusiveZone, IdlePolicy, LayerPatch, PlatformCx, SurfaceEvent, SurfaceId,
        WakeReason,
    };
    use zgui_runtime::{App, AppError, LayerOptions, Runtime};
    use zgui_view::{BuildCx, IntoView, View};

    /// The top of the area sway gives its workspace at each step, in order.
    static TOPS: Mutex<Vec<Option<i64>>> = Mutex::new(Vec::new());

    /// The heights of the resizes the application heard, in device pixels.
    static RESIZED: Mutex<Vec<f32>> = Mutex::new(Vec::new());

    /// How many frames the bar has drawn.
    static DRAWN: AtomicU64 = AtomicU64::new(0);

    /// The room the bar reserves when it opens.
    const FIRST: f32 = 34.0;

    /// The room it reserves after the change.
    const SECOND: f32 = 50.0;

    /// What the program is doing.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Step {
        /// Waiting for the first frames.
        Opening,
        /// The reserved room was read; the change has been asked for.
        Changed,
        /// The bar has been hidden and the show is on its way.
        Hidden,
        /// The bar has been shown again and is drawing.
        Shown,
    }

    /// Drives the bar through the steps.
    struct Drive {
        /// The application.
        inner: Runtime,
        /// Where it is.
        step: Step,
        /// The frame the current step began at.
        since: u64,
        /// How a thread of this program reaches the loop.
        waker: Option<std::sync::Arc<dyn zgui_platform::Waker>>,
    }

    impl Drive {
        /// Reads the top of the workspace area once the compositor has had a few frames to settle.
        fn settled(&self, frames: u64) -> bool {
            frames >= self.since + 6
        }
    }

    impl AppHandler for Drive {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            self.waker.get_or_insert_with(|| cx.waker());
            self.inner.surfaces_available(cx);
        }

        fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
            let drawn = matches!(event, SurfaceEvent::RedrawRequested);
            if let SurfaceEvent::Resized(size) = &event {
                RESIZED
                    .lock()
                    .expect("the record is not poisoned")
                    .push(size.height.0);
            }
            self.inner.surface_event(cx, surface, event);
            if !drawn {
                return;
            }
            let frames = DRAWN.fetch_add(1, Ordering::Relaxed) + 1;
            let Some(bar) = cx.surface(surface) else {
                return;
            };
            match self.step {
                Step::Opening if self.settled(frames) => {
                    record_top();
                    let patch = LayerPatch::new()
                        .with_exclusive_zone(ExclusiveZone::Reserve(CssPx(SECOND)));
                    bar.set_layer_placement(&patch)
                        .expect("a shell layer takes a new placement");
                    let _ = bar.request_size(Size::new(CssPx(0.0), CssPx(SECOND)));
                    // Between pixels, as a fractional scale leaves them. The regions ride the same
                    // frame, and a region the compositor refused would end the connection.
                    let card = Rect::new(
                        Point::new(CssPx(10.4), CssPx(2.6)),
                        Size::new(CssPx(120.3), CssPx(20.2)),
                    );
                    bar.set_input_region(Some(&[card]))
                        .expect("every surface takes an input region");
                    bar.set_opaque_region(Some(&[card]));
                    self.step = Step::Changed;
                    self.since = frames;
                }
                Step::Changed if self.settled(frames) => {
                    record_top();
                    bar.set_visible(false);
                    self.step = Step::Hidden;
                    if let Some(waker) = self.waker.clone() {
                        support::wake_after(waker, Duration::from_millis(300), || {
                            WakeReason::AppWork
                        });
                    }
                    return;
                }
                Step::Shown if self.settled(frames) => {
                    record_top();
                    cx.request_exit();
                    return;
                }
                _ => {}
            }
            // A still document asks for one frame, so each frame asks for the next.
            bar.request_redraw();
        }

        fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
            if self.step == Step::Hidden && matches!(reason, WakeReason::AppWork) {
                if let Some(bar) = cx.surfaces().first() {
                    bar.set_visible(true);
                }
                self.step = Step::Shown;
                self.since = DRAWN.load(Ordering::Relaxed);
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

    /// Records the top of the area sway gives the focused workspace.
    fn record_top() {
        TOPS.lock()
            .expect("the record is not poisoned")
            .push(workspace_top());
    }

    /// The top of the area sway gives its first workspace, when sway is the compositor.
    fn workspace_top() -> Option<i64> {
        std::env::var_os("SWAYSOCK")?;
        let done = std::process::Command::new("swaymsg")
            .args(["-r", "-t", "get_workspaces"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&done.stdout);
        let rect = text.find("\"rect\"")?;
        let after = &text[rect..];
        let y = after.find("\"y\":")? + 4;
        let digits: String = after[y..]
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        digits.parse().ok()
    }

    /// A bar across the top of the screen.
    fn bar() -> Result<Runtime, AppError> {
        App::new()
            .with_application_id(support::APP_ID)
            .with_layer(
                LayerOptions::top()
                    .with_anchors(Edges::new(true, true, false, true))
                    .with_size(0.0, FIRST)
                    .with_exclusive_zone(ExclusiveZone::Reserve(CssPx(FIRST)))
                    .with_namespace("zgui-test-bar"),
            )
            .with_stylesheet("root { display: block; height: 34px }")
            .with_renderer(support::renderers())
            .into_handler(|cx: &mut BuildCx<'_>| {
                Box::new(zgui_elements::column().class("root").into_view().build(cx))
            })
    }

    pub(super) fn main() {
        const PROPERTY: &str = "a shell layer keeps its placement through a change and a remap";

        if !support::require_nested(PROPERTY) {
            return;
        }

        support::tracing();
        support::watchdog(PROPERTY);

        let inner = match bar() {
            Ok(runtime) => runtime,
            Err(error) => {
                support::skipped(PROPERTY, &format!("the runtime would not install: {error}"));
                return;
            }
        };
        let handler = Drive {
            inner,
            step: Step::Opening,
            since: 0,
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
        let resized = RESIZED.lock().expect("the record is not poisoned").clone();
        assert!(
            resized.contains(&SECOND),
            "the application heard no resize to the requested height: {resized:?}"
        );
        let tops = TOPS.lock().expect("the record is not poisoned").clone();
        assert_eq!(tops.len(), 3, "the bar did not reach every step: {tops:?}");
        if tops.iter().any(Option::is_none) {
            support::skipped(PROPERTY, "only sway reports the room a layer reserves");
            return;
        }
        let reserved: Vec<i64> = tops.into_iter().flatten().collect();
        assert_eq!(
            reserved,
            [FIRST as i64, SECOND as i64, SECOND as i64],
            "the room reserved at opening, after the change and after the remap"
        );
        support::passed(
            PROPERTY,
            "34 pixels reserved, 50 after the change, and 50 again after the remap",
        );
    }
}
