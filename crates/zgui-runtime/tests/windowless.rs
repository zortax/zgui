//! An application that starts with no window and opens its windows itself.
//!
//! A desktop shell opens one bar for each monitor and a pop-over on demand, and no one of them is
//! the main window. Nothing may draw before the application asks, and work above the windows must
//! still run while no window is open, because that work is what opens the next one.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_platform::{ExclusiveZone, PlatformCx, SurfaceRole};
use zgui_platform_headless::Harness;
use zgui_reactive::prelude::{Get, Set};
use zgui_runtime::windows::{LayerOptions, WindowOptions, use_windows};
use zgui_runtime::{App, ExitPolicy};
use zgui_view::BuildCx;

/// A shell window holding an empty column.
fn open_bar() {
    use_windows().open(
        WindowOptions::layer(
            LayerOptions::top()
                .with_size(0.0, 34.0)
                .with_exclusive_zone(ExclusiveZone::Reserve(zgui_geom::CssPx(34.0)))
                .with_namespace("bar"),
        ),
        zgui_elements::column,
    );
}

/// An application with no primary window, set up by `setup`.
fn windowless(setup: impl FnOnce() + 'static) -> Harness<zgui_runtime::Runtime> {
    let handler = App::new()
        .without_primary_window()
        .with_exit_policy(ExitPolicy::Explicit)
        .with_renderer(Box::new(support::capture))
        .with_context(setup)
        .into_handler(|_: &mut BuildCx<'_>| -> Box<dyn zgui_view::Anchor> {
            panic!("the view of a primary window was built")
        })
        .expect("the reactive runtime installs");
    Harness::new(handler)
}

#[test]
fn a_window_asked_for_above_the_windows_opens_with_no_primary_window_beside_it() {
    let mut harness = windowless(open_bar);
    harness.settle(8);

    let surfaces = harness.platform().offscreens();
    assert_eq!(
        surfaces.len(),
        1,
        "only the window the application asked for"
    );
    let attributes = surfaces[0].requested_attributes();
    let SurfaceRole::Layer(placement) = attributes.role else {
        panic!("the window is a shell window");
    };
    assert_eq!(placement.namespace.as_deref(), Some("bar"));
    assert_eq!(
        placement.exclusive_zone,
        ExclusiveZone::Reserve(zgui_geom::CssPx(34.0))
    );
    assert!(!harness.platform().is_exiting());
    harness.shut_down();
}

#[test]
fn work_above_the_windows_runs_while_no_window_is_open() {
    let slot: Rc<Cell<Option<zgui_reactive::RwSignal<bool>>>> = Rc::new(Cell::new(None));
    let handed = Rc::clone(&slot);
    let mut harness = windowless(move || {
        let wanted = zgui_reactive::RwSignal::new(false);
        handed.set(Some(wanted));
        core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
            if wanted.get() {
                open_bar();
            }
        }));
    });
    harness.settle(8);
    assert!(
        harness.platform().offscreens().is_empty(),
        "nothing opens before the application asks"
    );
    assert!(
        !harness.platform().is_exiting(),
        "an explicit policy keeps an application with no window running"
    );

    // Written from outside any frame, with no window to run one.
    slot.get().expect("the setup ran").set(true);
    harness.settle(8);

    assert_eq!(
        harness.platform().offscreens().len(),
        1,
        "the effect ran, so the window it asked for opened"
    );
    harness.shut_down();
}
