//! Two overlays observing one scroll container, and what happens when one of them goes.
//!
//! This is the ordinary case rather than a contrived one. A popover and a virtualised list under
//! the same scroll container both need that container's `(offset, content, scrollport)` — the
//! popover to follow its anchor, the list to decide which rows exist — so two observations of one
//! `(node, ScrollPosition)` is what a normal screen looks like.
//!
//! The failure it exists to prevent is not a wrong value. If repeated calls handed back literally
//! the same signal, that signal's arena entry would belong to whichever scope asked *first*, and
//! that scope's cleanup would dispose it — leaving the second observer holding a disposed signal
//! whose next read panics, in a component that did nothing wrong and cannot see the other one. So
//! the registration is shared and refcounted while each caller's handle is its own, and both
//! unmount orders are checked because a mechanism that only survives one of them survives neither.

use std::rc::Rc;

use zgui_geom::{Device, DevicePx, Point, Size};
use zgui_interned::ElementName;
use zgui_reactive::Mounted;
use zgui_reactive::prelude::Get;
use zgui_testkit_view::Window;
use zgui_view::{Dom, NodeRef, ScrollPosition};

/// A scroll position `down` device pixels into a thousand pixels of content.
fn scrolled(down: f32) -> ScrollPosition {
    ScrollPosition {
        offset: Point::new(DevicePx(0.0), DevicePx(down)),
        content_size: Size::<DevicePx, Device>::new(DevicePx(400.0), DevicePx(1_000.0)),
        scrollport: Size::new(DevicePx(400.0), DevicePx(300.0)),
    }
}

/// One scroll container with two overlays under it, each observing its scroll position.
struct Overlays {
    /// The window everything is built in.
    window: Window,
    /// The container being observed.
    container: zgui_view::NodeId,
    /// The scope the first overlay lives in.
    first_scope: Mounted,
    /// The scope the second one lives in.
    second_scope: Mounted,
    /// What the first overlay reads.
    first: zgui_reactive::Signal<ScrollPosition, zgui_reactive::LocalStorage>,
    /// What the second one reads.
    second: zgui_reactive::Signal<ScrollPosition, zgui_reactive::LocalStorage>,
}

/// Builds the container and the two overlays, each in a scope of its own.
fn overlays() -> Overlays {
    let window = Window::open();
    let container = window.dom.create_element(ElementName::new("scroll"));
    window.dom.insert(window.root, container, None);

    let node_ref = window.scope.with(NodeRef::new);
    node_ref.bind(container, &window.dom_handle, &window.host_handle);

    // Two overlays are two components, so two scopes. Both are children of the window's, which is
    // what a portalled overlay actually is.
    let first_scope = window.scope.with(Mounted::new);
    let second_scope = window.scope.with(Mounted::new);
    let first = window
        .scope
        .with(|| first_scope.with(|| node_ref.observe_scroll()));
    let second = window
        .scope
        .with(|| second_scope.with(|| node_ref.observe_scroll()));

    Overlays {
        window,
        container,
        first_scope,
        second_scope,
        first,
        second,
    }
}

#[test]
fn two_observers_of_one_node_survive_the_first_unmount() {
    let stage = overlays();
    assert_eq!(
        stage.window.dom.tree().observation_count(),
        1,
        "two observers of one quantity cost the frame one registration"
    );

    stage.window.dom.deliver(
        stage.container,
        zgui_view::ObservedValue::ScrollPosition(scrolled(120.0)),
    );
    assert_eq!(stage.first.get().offset.y, DevicePx(120.0));
    assert_eq!(stage.second.get().offset.y, DevicePx(120.0));

    stage.first_scope.unmount();
    assert_eq!(
        stage.window.dom.tree().observation_count(),
        1,
        "one overlay went and took the other's registration with it"
    );

    stage.window.dom.deliver(
        stage.container,
        zgui_view::ObservedValue::ScrollPosition(scrolled(240.0)),
    );
    assert_eq!(
        stage.second.get().offset.y,
        DevicePx(240.0),
        "the surviving overlay stopped hearing about the scroll it is following"
    );

    stage.second_scope.unmount();
    assert_eq!(stage.window.dom.tree().observation_count(), 0);
    stage.window.scope.unmount();
}

#[test]
fn two_observers_of_one_node_survive_the_second_unmount() {
    // The order-reversed twin. A shared arena-backed signal is registered with whichever scope
    // asked first, so unmounting the *second* observer leaves the first one working by accident:
    // exactly one of these two tests would pass, and it would be this one.
    let stage = overlays();

    stage.window.dom.deliver(
        stage.container,
        zgui_view::ObservedValue::ScrollPosition(scrolled(120.0)),
    );
    assert_eq!(stage.first.get().offset.y, DevicePx(120.0));
    assert_eq!(stage.second.get().offset.y, DevicePx(120.0));

    stage.second_scope.unmount();
    assert_eq!(stage.window.dom.tree().observation_count(), 1);

    stage.window.dom.deliver(
        stage.container,
        zgui_view::ObservedValue::ScrollPosition(scrolled(240.0)),
    );
    assert_eq!(
        stage.first.get().offset.y,
        DevicePx(240.0),
        "the overlay that asked first stopped hearing about the scroll when the other one went"
    );

    stage.first_scope.unmount();
    assert_eq!(stage.window.dom.tree().observation_count(), 0);
    stage.window.scope.unmount();
}

#[test]
fn an_unobserved_container_costs_the_frame_nothing() {
    let window = Window::open();
    let container = window.dom.create_element(ElementName::new("scroll"));
    window.dom.insert(window.root, container, None);
    assert_eq!(
        window.dom.tree().observation_count(),
        0,
        "the entry test a frame skips its whole delivery pass on is this one"
    );
    let _ = Rc::strong_count(&window.dom);
    window.scope.unmount();
}
