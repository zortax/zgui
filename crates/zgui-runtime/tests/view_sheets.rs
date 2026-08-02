//! Two answers a view asks a real window for, and which nothing else in the loop would notice were
//! missing.
//!
//! **A style sheet a view installed for itself.** A component library carries its own rules, and
//! the program using it never wrote them down — so the rules arrive from a component's body rather
//! than from the application's sheet. The assertion is on the *layout*, not on a table of sheets:
//! a sheet that was installed and never cascaded is a component that renders unstyled, and only a
//! frame can tell the difference.
//!
//! **How many animations are running on a node.** Everything that keeps content mounted through
//! its exit animation is written against that number, and a host that always answered zero would
//! make every one of them take the "nothing is running" branch and unmount before a single frame
//! of the exit was drawn — with every test about the animation itself still passing, because the
//! animation is fine.

mod support;

use std::time::Duration;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::Set;
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// Only the frame around the content: everything else arrives from the view itself.
const FRAME_CSS: &str = "root { display: block; width: 400px; height: 300px }";

/// What a component installs for itself, the way a library's `style!` block does.
const COMPONENT_CSS: &str = ".panel { display: block; width: 120px; height: 60px }";

/// The same class, wider, as a theme that has been changed.
const RETHEMED_CSS: &str = ".panel { display: block; width: 250px; height: 60px }";

/// A little more than one frame at the surface's refresh rate.
const FRAME: Duration = Duration::from_millis(17);

#[test]
fn a_sheet_a_view_installed_reaches_the_cascade_in_the_frame_it_was_asked_for() {
    let mut harness = support::app(FRAME_CSS, move |cx: &mut BuildCx<'_>| {
        // Exactly what a component's body does.
        zgui_view::install_stylesheet("panel", COMPONENT_CSS);
        let view = zgui_elements::r#box().class("panel").into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);

    // The class is the view's own, and only the view's sheet gives it a width.
    let window = harness.app().windows().first().expect("a window");
    let laid_out: Vec<f32> = {
        let layout = window.layout().borrow();
        layout
            .keys()
            .into_iter()
            .filter_map(|key| layout.layout_of(key))
            .map(|resolved| resolved.border_box().size.width.0)
            .collect()
    };
    assert!(
        laid_out.contains(&120.0),
        "no box was 120px wide, so the view's own sheet never reached the cascade: {laid_out:?}"
    );
}

#[test]
fn re_installing_a_sheet_under_one_name_restyles_rather_than_adding_a_second() {
    let widen = RwSignal::new(false);
    let mut harness = support::app(FRAME_CSS, move |cx: &mut BuildCx<'_>| {
        let view = zgui_view::flow::Dynamic::new(move || {
            use zgui_reactive::prelude::Get;
            let css = if widen.get() {
                RETHEMED_CSS
            } else {
                COMPONENT_CSS
            };
            zgui_view::install_stylesheet("panel", css);
            zgui_view::AnyView::new(zgui_elements::r#box().class("panel"))
        })
        .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);

    let widths = |harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>| -> Vec<f32> {
        let window = harness.app().windows().first().expect("a window");
        let layout = window.layout().borrow();
        layout
            .keys()
            .into_iter()
            .filter_map(|key| layout.layout_of(key))
            .map(|resolved| resolved.border_box().size.width.0)
            .collect()
    };
    assert!(widths(&harness).contains(&120.0), "{:?}", widths(&harness));

    widen.set(true);
    harness.settle(8);

    let after = widths(&harness);
    assert!(
        after.contains(&250.0),
        "replacing the sheet's text did not restyle anything: {after:?}"
    );
    assert!(
        !after.contains(&120.0),
        "the old sheet is still winning somewhere: {after:?}"
    );
}

/// A button whose background transitions on hover, so that something is genuinely running.
const HOVER_CSS: &str = "root { display: block; width: 400px; height: 300px }
                         .btn { display: block; width: 200px; height: 100px;
                                background-color: rgb(16, 16, 16);
                                transition: background-color 400ms linear }
                         .btn:hover { background-color: rgb(240, 240, 240) }";

#[test]
fn a_view_can_see_that_a_transition_is_running_on_its_own_node() {
    // Without this the answer is always zero, and everything written against it — keeping content
    // mounted until its exit animation ends above all — takes the wrong branch every time.
    let held: std::rc::Rc<std::cell::Cell<Option<NodeRef>>> = std::rc::Rc::default();
    let recorded = std::rc::Rc::clone(&held);
    let mut harness = support::app(HOVER_CSS, move |cx: &mut BuildCx<'_>| {
        let handle = NodeRef::new();
        recorded.set(Some(handle));
        let view = zgui_elements::r#box()
            .class("btn")
            .node_ref(handle)
            .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);

    let button = held.get().expect("the view was built");
    assert_eq!(
        button.running_animations(),
        0,
        "nothing is running before the pointer arrives"
    );

    harness.deliver_to_first(SurfaceEvent::Pointer {
        event: PointerEvent::mouse(Point::new(CssPx(20.0), CssPx(20.0))),
        action: PointerAction::Moved,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(4);
    harness.advance(FRAME);
    harness.settle(4);

    assert_eq!(
        button.running_animations(),
        1,
        "the hover transition is running and the view cannot see it"
    );

    // And it stops being true when the transition finishes, so this is not a number that only ever
    // grows.
    for _ in 0..40 {
        harness.advance(FRAME);
        harness.settle(2);
    }
    assert_eq!(button.running_animations(), 0);
}
