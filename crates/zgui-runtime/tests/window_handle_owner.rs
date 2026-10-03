//! That a window's handle keeps its state for as long as the window, whoever opened it.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::{Device, DevicePx, Size};
use zgui_platform::SurfaceEvent;
use zgui_reactive::Mounted;
use zgui_reactive::prelude::*;
use zgui_runtime::windows::{WindowHandle, WindowOptions, use_windows};
use zgui_view::{BuildCx, IntoView, View};

/// The stylesheet every window here is styled by.
const SHEET: &str = "column { display: block }";

#[test]
fn a_window_survives_the_scope_that_opened_it() {
    let opener: Rc<RefCell<Option<(Mounted, WindowHandle)>>> = Rc::default();
    let slot = Rc::clone(&opener);
    let mut harness = support::app(SHEET, move |cx: &mut BuildCx<'_>| {
        // A component that opens a window and is then unmounted, while the window stays open.
        let scope = Mounted::new();
        let handle = scope.with(|| {
            use_windows().open(
                WindowOptions::new("child").with_size(400.0, 300.0),
                zgui_elements::column,
            )
        });
        *slot.borrow_mut() = Some((scope, handle));
        Box::new(zgui_elements::column().into_view().build(cx))
    });
    harness.settle(8);
    assert_eq!(harness.app().windows().len(), 2);

    let (scope, handle) = opener.borrow_mut().take().expect("the view was built");
    scope.unmount();

    let child = harness
        .platform()
        .offscreens()
        .get(1)
        .map(|surface| zgui_platform::Surface::id(surface.as_ref()))
        .expect("the child window opened");
    harness.deliver(
        child,
        SurfaceEvent::Resized(Size::<DevicePx, Device>::new(
            DevicePx(640.0),
            DevicePx(480.0),
        )),
    );
    harness.settle(4);

    let size = handle.size().get_untracked();
    assert_eq!((size.width.0, size.height.0), (640.0, 480.0));
}
