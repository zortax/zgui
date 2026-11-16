//! The keyboard and the pointer, limited to a set of windows.
//!
//! A grab gives the platform the surfaces of its open windows, follows the set as windows are
//! added, removed and closed, and tells the application when the desktop clears it. Dropping it
//! destroys the platform's grab. A desktop that cannot grab makes no grab.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_platform::{PlatformCapabilities, SurfaceId};
use zgui_platform_headless::{Harness, Headless};
use zgui_reactive::prelude::GetUntracked;
use zgui_runtime::grab::FocusGrab;
use zgui_runtime::windows::{LayerOptions, WindowHandle, WindowOptions};
use zgui_runtime::{App, Runtime};
use zgui_view::{BuildCx, IntoView, View};

/// An application with a bar and two pop-overs, over a platform that can grab when `grabs` says.
fn shell(grabs: bool) -> (Harness<Runtime>, WindowHandle, WindowHandle, WindowHandle) {
    let handler = App::new()
        .with_layer(LayerOptions::top().with_size(0.0, 34.0))
        .with_renderer(Box::new(support::capture))
        .into_handler(|cx: &mut BuildCx<'_>| {
            Box::new(zgui_elements::column().into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut capabilities = PlatformCapabilities::none();
    capabilities.focus_grab = grabs;
    let platform = Headless::new().with_capabilities(capabilities);
    let mut harness = Harness::over(platform, handler);
    harness.settle(8);
    let bar = harness.app().windows()[0].handle().clone();
    let open = |harness: &Harness<Runtime>| {
        harness.app().scope().with(|| {
            zgui_runtime::use_windows().open(
                WindowOptions::layer(LayerOptions::top().with_size(320.0, 200.0)),
                zgui_elements::column,
            )
        })
    };
    let first = open(&harness);
    let second = open(&harness);
    harness.settle(8);
    (harness, bar, first, second)
}

/// The surface `window` is open on.
fn surface_of(harness: &Harness<Runtime>, window: &WindowHandle) -> SurfaceId {
    harness
        .app()
        .windows()
        .iter()
        .find(|open| open.handle() == window)
        .map(|open| open.surface().id())
        .expect("the window is open")
}

/// A grab over `windows`, made where the application's code runs.
fn grab(harness: &Harness<Runtime>, windows: &[&WindowHandle]) -> Option<FocusGrab> {
    harness.app().scope().with(|| FocusGrab::new(windows))
}

#[test]
fn a_desktop_that_cannot_grab_makes_no_grab() {
    let (mut harness, _bar, first, _second) = shell(false);
    assert!(grab(&harness, &[&first]).is_none());
    harness.settle(8);
    assert!(harness.platform().focus_grabs().is_empty());
    harness.shut_down();
}

#[test]
fn a_grab_follows_its_windows_and_lets_go_when_dropped() {
    let (mut harness, _bar, first, second) = shell(true);
    let held = grab(&harness, &[&first]).expect("this desktop can grab");
    assert!(
        !held.active().get_untracked(),
        "the grab starts on the next turn"
    );
    harness.settle(8);
    let [platform] = harness.platform().focus_grabs()[..] else {
        panic!("one grab was made");
    };
    let first_surface = surface_of(&harness, &first);
    let second_surface = surface_of(&harness, &second);
    assert_eq!(
        harness.platform().focus_grab(platform),
        Some(vec![first_surface])
    );
    assert!(held.active().get_untracked());

    held.add(&second);
    held.add(&second);
    harness.settle(8);
    assert_eq!(
        harness.platform().focus_grab(platform),
        Some(vec![first_surface, second_surface])
    );

    held.remove(&first);
    harness.settle(8);
    assert_eq!(
        harness.platform().focus_grab(platform),
        Some(vec![second_surface])
    );

    // A window of the set that closes leaves it, and the grab stops with its last window.
    second.close();
    harness.settle(8);
    assert_eq!(harness.platform().focus_grab(platform), Some(Vec::new()));
    assert!(!held.active().get_untracked());

    drop(held);
    harness.settle(8);
    assert!(
        harness.platform().focus_grabs().is_empty(),
        "dropping the grab destroyed it"
    );
    harness.shut_down();
}

#[test]
fn a_clear_reaches_the_application_and_an_add_starts_the_grab_again() {
    let (mut harness, _bar, first, second) = shell(true);
    let held = grab(&harness, &[&first, &second]).expect("this desktop can grab");
    let clears = Rc::new(Cell::new(0));
    let guard = {
        let clears = Rc::clone(&clears);
        held.on_cleared(move || clears.set(clears.get() + 1))
    };
    harness.settle(8);
    let [platform] = harness.platform().focus_grabs()[..] else {
        panic!("one grab was made");
    };

    // A press outside the set.
    harness.platform().clear_focus_grab(platform);
    harness.settle(8);
    assert_eq!(clears.get(), 1);
    assert!(!held.active().get_untracked());
    assert!(held.windows().is_empty(), "a clear empties the set");

    held.add(&first);
    harness.settle(8);
    assert_eq!(
        harness.platform().focus_grab(platform),
        Some(vec![surface_of(&harness, &first)])
    );
    assert!(held.active().get_untracked());

    drop(guard);
    harness.platform().clear_focus_grab(platform);
    harness.settle(8);
    assert_eq!(clears.get(), 1, "the guard took the callback away");
    harness.shut_down();
}

#[test]
fn a_window_that_is_not_open_yet_joins_the_grab_when_it_opens() {
    let (mut harness, _bar, _first, _second) = shell(true);
    let later = harness.app().scope().with(|| {
        zgui_runtime::use_windows().open(
            WindowOptions::layer(LayerOptions::top().with_size(100.0, 100.0)),
            zgui_elements::column,
        )
    });
    let held = grab(&harness, &[&later]).expect("this desktop can grab");
    harness.settle(8);
    let [platform] = harness.platform().focus_grabs()[..] else {
        panic!("one grab was made");
    };
    assert_eq!(
        harness.platform().focus_grab(platform),
        Some(vec![surface_of(&harness, &later)])
    );
    drop(held);
    harness.shut_down();
}
