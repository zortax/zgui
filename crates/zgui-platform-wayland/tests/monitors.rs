//! Outputs come and go, and a shell window on one that goes closes for that reason.
//!
//! Four properties, each driven by the compositor:
//!
//! * **A new output reaches the application.** The loop is woken with `MonitorsChanged`, and the
//!   list holds the output with its identifier, its name and its logical extent.
//! * **A shell window opens on an output named by its identifier.**
//! * **It opens at the scale of that output.** The scale is read when the window opens, before the
//!   compositor has said anything about its surface. The renderer and the first frame are made at
//!   this scale, so a window that opened at 1x on a 2x output draws its first frame twice.
//! * **A shell window whose output is unplugged closes with `OutputGone`.**
//!
//! Only sway can add and remove an output on request. On another compositor this program says it
//! was skipped.

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

    use zgui_geom::Edges;
    use zgui_platform::{
        AppHandler, CloseReason, IdlePolicy, MonitorInfo, PlatformCx, SurfaceEvent, SurfaceId,
        WakeReason,
    };
    use zgui_runtime::{
        App, AppError, ExitPolicy, LayerOptions, Runtime, WindowHandle, WindowOptions,
    };
    use zgui_view::prelude::GetUntracked;
    use zgui_view::{BuildCx, IntoView, View};

    /// What the program saw, in the order it saw it.
    #[derive(Clone, Debug, Default)]
    struct Seen {
        /// The outputs at start-up.
        before: Vec<MonitorInfo>,
        /// The output sway added.
        added: Option<MonitorInfo>,
        /// The scale the window on the added output opened at.
        first_scale: Option<f32>,
        /// Why that window closed.
        closed: Option<CloseReason>,
        /// The outputs after the unplug.
        after: Vec<MonitorInfo>,
    }

    /// What the program saw.
    static SEEN: Mutex<Option<Seen>> = Mutex::new(None);

    /// What the program is doing.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Step {
        /// An output was asked for.
        Adding,
        /// The new output was asked to scale by two.
        Scaling,
        /// A window was opened on the new output.
        Opened,
        /// The new output was unplugged.
        Unplugged,
    }

    /// Drives the outputs through the steps.
    struct Drive {
        /// The application.
        inner: Runtime,
        /// Where it is.
        step: Step,
        /// What it saw.
        seen: Seen,
        /// The window on the added output.
        window: Option<WindowHandle>,
    }

    impl Drive {
        /// Finishes the loop and keeps what was seen.
        fn finish(&mut self, cx: &dyn PlatformCx) {
            self.seen.after = cx.monitors();
            *SEEN.lock().expect("the record is not poisoned") = Some(self.seen.clone());
            cx.request_exit();
        }

        /// The output sway added, as the platform now describes it.
        fn added(&self, cx: &dyn PlatformCx) -> Option<MonitorInfo> {
            cx.monitors().into_iter().find(|monitor| {
                !self
                    .seen
                    .before
                    .iter()
                    .any(|before| before.id == monitor.id)
            })
        }
    }

    impl AppHandler for Drive {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            self.inner.surfaces_available(cx);
            self.seen.before = cx.monitors();
            sway(&["create_output"]);
        }

        fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
            let drawn = matches!(event, SurfaceEvent::RedrawRequested);
            self.inner.surface_event(cx, surface, event);
            let Some(window) = self.window.clone() else {
                return;
            };
            if self.step == Step::Unplugged {
                if let Some(reason) = window.close_reason().get_untracked() {
                    self.seen.closed = Some(reason);
                    self.finish(cx);
                }
                return;
            }
            if !drawn || !window.is_open() {
                return;
            }
            // Its first frame has been drawn, so the output can go.
            if let Some(name) = self
                .seen
                .added
                .as_ref()
                .and_then(|added| added.name.clone())
            {
                self.step = Step::Unplugged;
                sway(&["output", &name, "unplug"]);
            }
        }

        fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
            let monitors = matches!(reason, WakeReason::MonitorsChanged);
            self.inner.wake(cx, reason);
            // The scale the window opened at, read before the compositor has said anything about its
            // surface. The renderer and the first frame are made at this scale.
            if let Some(window) = self.window.as_ref().filter(|window| window.is_open())
                && self.seen.first_scale.is_none()
            {
                self.seen.first_scale = Some(window.scale().get_untracked());
            }
            if !monitors {
                return;
            }
            match self.step {
                Step::Adding => {
                    if let Some(added) = self.added(cx)
                        && let Some(name) = added.name.clone()
                    {
                        self.seen.added = Some(added);
                        self.step = Step::Scaling;
                        sway(&["output", &name, "scale", "2"]);
                    }
                }
                Step::Scaling => {
                    let Some(added) = self.added(cx).filter(|added| added.scale_factor == 2.0)
                    else {
                        return;
                    };
                    let id = added.id.expect("a wayland output has an identifier");
                    self.seen.added = Some(added);
                    self.step = Step::Opened;
                    let window = self.inner.scope().with(|| {
                        zgui_runtime::use_windows().open(
                            WindowOptions::layer(
                                LayerOptions::top()
                                    .with_anchors(Edges::new(true, true, false, true))
                                    .with_size(0.0, 30.0)
                                    .with_namespace("zgui-test-output-bar")
                                    .with_monitor(id),
                            ),
                            || zgui_elements::column().class("root"),
                        )
                    });
                    self.window = Some(window);
                }
                Step::Unplugged => {
                    if let Some(reason) = self
                        .window
                        .as_ref()
                        .and_then(|window| window.close_reason().get_untracked())
                    {
                        self.seen.closed = Some(reason);
                        self.finish(cx);
                    }
                }
                Step::Opened => {}
            }
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

    /// Asks sway for something, when sway is the compositor.
    fn sway(arguments: &[&str]) {
        let _ = std::process::Command::new("swaymsg")
            .args(arguments)
            .output();
    }

    /// A bar on whichever output the desktop chooses, which keeps the application running.
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
            .with_stylesheet("root { display: block; height: 30px }")
            .with_renderer(support::renderers())
            .into_handler(|cx: &mut BuildCx<'_>| {
                Box::new(zgui_elements::column().class("root").into_view().build(cx))
            })
    }

    pub(super) fn main() {
        const PROPERTY: &str = "outputs come and go, and a shell window on one that goes closes";

        if !support::require_nested(PROPERTY) {
            return;
        }

        support::tracing();
        support::watchdog(PROPERTY);

        if std::env::var_os("SWAYSOCK").is_none() {
            support::skipped(PROPERTY, "only sway adds and removes an output on request");
            return;
        }
        let inner = match application() {
            Ok(runtime) => runtime,
            Err(error) => {
                support::skipped(PROPERTY, &format!("the runtime would not install: {error}"));
                return;
            }
        };
        let handler = Drive {
            inner,
            step: Step::Adding,
            seen: Seen::default(),
            window: None,
        };
        let Some(mut app) = support::loop_for(PROPERTY, handler) else {
            return;
        };
        if let Err(error) = app.run() {
            panic!("the loop stopped: {error}");
        }
        if support::NO_DEVICE.load(std::sync::atomic::Ordering::Relaxed) {
            support::skipped(
                PROPERTY,
                "this machine has no graphics device to draw through",
            );
            return;
        }

        let seen = SEEN
            .lock()
            .expect("the record is not poisoned")
            .clone()
            .expect("the program reached its last step");
        let added = seen.added.expect("the new output reached the application");
        assert!(added.id.is_some(), "the output has an identifier");
        assert!(added.name.is_some(), "the output has a name");
        let logical = added.logical.expect("the output has a logical extent");
        assert!(logical.size.width.0 > 0.0 && logical.size.height.0 > 0.0);
        assert_eq!(added.fractional_scale, Some(2.0));
        assert_eq!(
            seen.first_scale,
            Some(2.0),
            "the window opens at the scale of its output"
        );
        assert_eq!(seen.closed, Some(CloseReason::OutputGone));
        assert_eq!(
            seen.after.len(),
            seen.before.len(),
            "the unplugged output left the list: {:?}",
            seen.after
        );
        support::passed(
            PROPERTY,
            "the output arrived, the window drew at 2x, and it closed when the output went",
        );
    }
}
