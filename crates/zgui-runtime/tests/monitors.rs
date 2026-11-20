//! The outputs, as the application reads them, and why a window closed.
//!
//! The list of outputs is the same signal in every window and in the application's own scope,
//! and it changes when the platform says an output came or went. A shell window whose output went
//! away closes for good, and its handle says why.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::{DevicePx, Edges, Point, Size};
use zgui_platform::{CloseReason, MonitorId, MonitorInfo, Surface, SurfaceEvent, SurfaceRole};
use zgui_platform_headless::{Harness, Headless};
use zgui_reactive::prelude::*;
use zgui_runtime::windows::{LayerOptions, WindowHandle, WindowOptions};
use zgui_runtime::{App, Runtime};
use zgui_view::{BuildCx, IntoView, View};

/// An output called `name`, numbered `id`.
fn output(id: u64, name: &str) -> MonitorInfo {
    MonitorInfo::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(1920.0), DevicePx(1080.0)),
        1.0,
    )
    .with_id(MonitorId::new(id))
    .with_name(name)
}

thread_local! {
    /// The outputs as the bar's own view read them.
    static INSIDE: RefCell<Option<zgui_reactive::Signal<Vec<MonitorInfo>>>> =
        const { RefCell::new(None) };
    /// Whether the bar's view refuses a close the user asks for.
    static VETO: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// An application with a bar on `DP-1`, over a platform with `DP-1` and `DP-2`.
///
/// The bar's view keeps the outputs it reads, and refuses the user's close when `VETO` says so.
fn bar_on_first_output() -> (Harness<Runtime>, WindowHandle) {
    let handler = App::new()
        .with_layer(
            LayerOptions::top()
                .with_anchors(Edges::new(true, true, false, true))
                .with_size(0.0, 34.0)
                .with_monitor(MonitorId::new(1)),
        )
        .with_renderer(Box::new(support::capture))
        .into_handler(|cx: &mut BuildCx<'_>| {
            INSIDE.with(|inside| *inside.borrow_mut() = Some(zgui_runtime::use_monitors()));
            if VETO.with(std::cell::Cell::get) {
                std::mem::forget(zgui_runtime::on_close_request(|| {
                    zgui_runtime::CloseResponse::Veto
                }));
            }
            Box::new(zgui_elements::column().into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let platform = Headless::new().with_monitors(vec![output(1, "DP-1"), output(2, "DP-2")]);
    let mut harness = Harness::over(platform, handler);
    harness.settle(8);
    let bar = harness.app().windows()[0].handle().clone();
    (harness, bar)
}

/// The names of `monitors`, in order.
fn names(monitors: &[MonitorInfo]) -> Vec<String> {
    monitors
        .iter()
        .filter_map(|monitor| monitor.name.as_deref().map(str::to_owned))
        .collect()
}

#[test]
fn the_outputs_are_one_signal_in_every_window_and_above_them() {
    let (mut harness, _bar) = bar_on_first_output();
    let above = harness.app().scope().with(zgui_runtime::use_monitors);
    let inside = INSIDE
        .with(|inside| *inside.borrow())
        .expect("the bar's view read the outputs");
    assert_eq!(names(&above.get_untracked()), ["DP-1", "DP-2"]);
    assert_eq!(names(&inside.get_untracked()), ["DP-1", "DP-2"]);

    // An effect above the windows hears the change, as a shell that keeps one bar per output does.
    let heard = Rc::new(RefCell::new(Vec::new()));
    let _effect = harness.app().scope().with(|| {
        let heard = Rc::clone(&heard);
        zgui_reactive::RenderEffect::new(move |_| {
            heard.borrow_mut().push(names(&above.get()));
        })
    });
    harness.settle(8);
    harness.platform().set_monitors(vec![output(2, "DP-2")]);
    harness.settle(8);
    assert_eq!(names(&inside.get_untracked()), ["DP-2"]);
    assert_eq!(
        heard.borrow().last().map(Vec::as_slice),
        Some(["DP-2".to_owned()].as_slice())
    );

    // The same list again is not a change.
    let before = heard.borrow().len();
    harness.platform().set_monitors(vec![output(2, "DP-2")]);
    harness.settle(8);
    assert_eq!(heard.borrow().len(), before);
    harness.shut_down();
}

#[test]
fn a_shell_window_asks_for_the_output_it_was_given() {
    let (mut harness, _bar) = bar_on_first_output();
    let surface = harness.platform().offscreens()[0].clone();
    let SurfaceRole::Layer(placement) = surface.requested_attributes().role else {
        panic!("the bar is a shell window");
    };
    assert_eq!(placement.monitor, Some(MonitorId::new(1).into()));
    harness.shut_down();
}

#[test]
fn a_shell_window_whose_output_went_away_closes_and_says_why() {
    let (mut harness, bar) = bar_on_first_output();
    let other = harness.app().scope().with(|| {
        zgui_runtime::use_windows().open(
            WindowOptions::layer(LayerOptions::top().with_monitor("DP-2")),
            zgui_elements::column,
        )
    });
    harness.settle(8);
    assert_eq!(bar.close_reason().get_untracked(), None);

    let surface = harness.platform().offscreens()[0].id();
    harness.platform().set_monitors(vec![output(2, "DP-2")]);
    harness.deliver(surface, SurfaceEvent::Closed(CloseReason::OutputGone));
    harness.settle(8);

    assert!(!bar.is_open());
    assert_eq!(
        bar.close_reason().get_untracked(),
        Some(CloseReason::OutputGone)
    );
    assert!(other.is_open(), "the window on the other output stays");
    assert_eq!(
        harness.platform().offscreens().len(),
        1,
        "the surface was destroyed"
    );
    let open = harness
        .app()
        .scope()
        .with(|| zgui_runtime::use_windows().all());
    assert_eq!(open, std::slice::from_ref(&other));

    // The application closing its own window and the user closing one are told apart too.
    other.close();
    harness.settle(8);
    assert_eq!(
        other.close_reason().get_untracked(),
        Some(CloseReason::Application)
    );
}

#[test]
fn a_close_the_desktop_makes_cannot_be_refused() {
    VETO.with(|veto| veto.set(true));
    let (mut harness, bar) = bar_on_first_output();
    let surface = harness.platform().offscreens()[0].id();

    harness.deliver(surface, SurfaceEvent::CloseRequested);
    harness.settle(8);
    assert!(bar.is_open(), "the user's close was refused");

    harness.deliver(surface, SurfaceEvent::Closed(CloseReason::Desktop));
    harness.settle(8);
    assert!(!bar.is_open());
    assert_eq!(
        bar.close_reason().get_untracked(),
        Some(CloseReason::Desktop)
    );
}
