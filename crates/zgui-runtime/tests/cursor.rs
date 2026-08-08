//! Which cursor a window shows, and where a caret takes its colour from.
//!
//! Both are states a *surface* is told about rather than things a document holds, so neither is
//! visible in a fragment tree, a display list or a computed style. They are asserted here, against
//! what the surface was actually told, over a real window and a real cascade.

mod support;

use std::time::Duration;

use zgui_geom::{CssPx, Point};
use zgui_platform::{CursorStyle, SurfaceEvent};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// A little more than one frame at the surface's refresh rate.
const FRAME: Duration = Duration::from_millis(17);

/// Three boxes side by side, each asking for a different cursor.
const CSS: &str = "root { display: block; width: 400px; height: 300px; cursor: default }
                   .btn { display: block; position: absolute; left: 0; top: 0;
                          width: 100px; height: 100px; cursor: pointer }
                   .grip { display: block; position: absolute; left: 100px; top: 0;
                           width: 100px; height: 100px; cursor: col-resize }
                   .plain { display: block; position: absolute; left: 200px; top: 0;
                            width: 100px; height: 100px }
                   .label { display: block; width: 40px; height: 20px }";

/// Moves the pointer to a point in the window and runs a frame.
fn point_at(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, x: f32, y: f32) {
    harness.deliver_to_first(SurfaceEvent::Pointer {
        event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(y))),
        action: PointerAction::Moved,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(4);
    harness.advance(FRAME);
    harness.settle(4);
}

/// The cursor the window was last told to show.
fn shown(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Option<CursorStyle> {
    harness.platform().offscreens().first()?.last_cursor()
}

#[test]
fn the_element_under_the_pointer_decides_the_cursor() {
    let mut harness = support::app(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let view = zgui_elements::r#box()
            .child(
                zgui_elements::r#box()
                    .class("btn")
                    // A child that says nothing about the cursor: the property is inherited, so
                    // the label inside a button shows the button's cursor and not the arrow.
                    .child(zgui_elements::r#box().class("label")),
            )
            .child(zgui_elements::r#box().class("grip"))
            .child(zgui_elements::r#box().class("plain"))
            .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);

    point_at(&mut harness, 20.0, 20.0);
    assert_eq!(
        shown(&harness),
        Some(CursorStyle::Pointer),
        "the pointer is over the button, whose own cursor is the hand",
    );

    point_at(&mut harness, 150.0, 20.0);
    assert_eq!(
        shown(&harness),
        Some(CursorStyle::ResizeColumn),
        "and over the grip, whose keyword names a resize arrow",
    );

    point_at(&mut harness, 250.0, 20.0);
    assert_eq!(
        shown(&harness),
        Some(CursorStyle::Default),
        "and over a box that says nothing, which inherits the root's arrow",
    );
}

#[test]
fn an_unmoved_pointer_costs_no_call_into_the_windowing_system() {
    // The cursor is set through the compositor and the pointer sits still over one element for most
    // of a session, so a frame that would ask for the cursor it is already showing asks for
    // nothing. Without this every frame of every animation carries a cursor call.
    let mut harness = support::app(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let view = zgui_elements::r#box()
            .child(zgui_elements::r#box().class("btn"))
            .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);

    point_at(&mut harness, 20.0, 20.0);
    let after_arriving = harness.platform().offscreens()[0].cursor_log().len();
    assert!(after_arriving > 0, "the arrival was never reported");

    for _ in 0..5 {
        point_at(&mut harness, 21.0, 21.0);
    }
    assert_eq!(
        harness.platform().offscreens()[0].cursor_log().len(),
        after_arriving,
        "the pointer stayed on the same element and the cursor was set again anyway",
    );
}

// ---- caret-color ------------------------------------------------------------------------------

/// A field whose caret is a colour of its own, and one whose caret follows its text.
const CARET_CSS: &str = "root { display: block; width: 400px; height: 300px }
                         .field { display: block; width: 200px; height: 40px;
                                  color: rgb(10, 20, 30) }
                         .field.tinted { caret-color: rgb(200, 100, 50) }";

/// The colour of the caret the window is currently planning, if it is planning one.
fn caret_color(
    harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) -> Option<zgui_color::Color> {
    let window = harness.app().windows().first()?;
    window.caret_color()
}

#[test]
fn a_caret_takes_its_own_colour_and_falls_back_to_the_text() {
    let tinted = zgui_reactive::RwSignal::new(false);
    let mut harness = support::app_with_text(CARET_CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_reactive::prelude::Get;
        use zgui_view::{IntoView, View};
        let view = zgui_elements::editor()
            .class("field")
            .class_toggle(zgui_interned::ClassName::new("tinted"), move || {
                tinted.get()
            })
            .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(8);

    harness.deliver_to_first(SurfaceEvent::Pointer {
        event: PointerEvent::mouse(Point::new(CssPx(20.0), CssPx(20.0))),
        action: PointerAction::Pressed,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(8);
    harness.advance(FRAME);
    harness.settle(8);

    let plain = caret_color(&harness).expect("a caret is planned in the focused field");
    assert_eq!(
        plain.components(),
        [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0],
        "`caret-color: auto` is the text's own colour",
    );

    use zgui_reactive::prelude::Set;
    tinted.set(true);
    harness.settle(8);
    harness.advance(FRAME);
    harness.settle(8);

    let written = caret_color(&harness).expect("still planning a caret");
    assert_eq!(
        written.components(),
        [200.0 / 255.0, 100.0 / 255.0, 50.0 / 255.0],
        "and a colour that was written is the colour that is drawn",
    );
}

// ---- user-select ------------------------------------------------------------------------------

/// A field, and a field that refuses selection.
const SELECT_CSS: &str = "root { display: block; width: 400px; height: 300px }
                          .field { display: block; width: 200px; height: 40px }
                          .quiet { display: block; user-select: none }";

/// A window whose field carries `on_field`, inside a panel carrying `on_panel`.
///
/// Returns the harness and a handle on the field, because what a press did is read off the field's
/// own selection: focus alone puts a caret at the beginning whether or not the press was allowed to
/// place one, so *whether* there is a caret measures nothing and *where* it is measures the press.
fn field_in(
    on_panel: &'static str,
    on_field: &'static str,
) -> (
    zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    zgui_view::NodeRef,
) {
    let field = zgui_view::NodeRef::new();
    let harness = support::app_with_text(SELECT_CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let view = zgui_elements::r#box()
            .class(on_panel)
            .child(
                zgui_elements::editor()
                    .node_ref(field)
                    .class("field")
                    .class(on_field)
                    .child("hello there"),
            )
            .into_view();
        Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
    });
    (harness, field)
}

/// Presses two characters into the field's text and runs a frame.
fn press(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>) {
    harness.settle(8);
    harness.deliver_to_first(SurfaceEvent::Pointer {
        event: PointerEvent::mouse(Point::new(CssPx(20.0), CssPx(10.0))),
        action: PointerAction::Pressed,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(8);
    harness.advance(FRAME);
    harness.settle(8);
}

/// A field that refuses selection does not move its caret for a press, and an ordinary one does.
///
/// Both halves, because the property is invisible without the control: a window in which no press
/// ever moved a caret would satisfy the first assertion while making the whole text interaction
/// dead.
#[test]
fn user_select_none_stops_a_press_placing_the_caret() {
    let (mut ordinary, field) = field_in("", "");
    press(&mut ordinary);
    let moved = field.selection().map(|range| range.start);
    assert!(
        moved.is_some_and(|start| start > 0),
        "a press in an ordinary field left the caret at the beginning: {moved:?}",
    );

    let (mut quiet, field) = field_in("", "quiet");
    press(&mut quiet);
    assert_eq!(
        field.selection().map(|range| range.start),
        Some(0),
        "the field refuses selection and a press moved the caret anyway",
    );
}

/// A container that refuses selection does not refuse it for an editable element inside it.
///
/// The one case in the property where an ancestor does *not* decide, and the specification says so
/// outright: `auto` on an editable element is settled by the element being editable and never by
/// what is above it. Without it, switching selection off on a panel — which is what a panel full of
/// labels and drag handles is written with — would take the fields in it with it, and a form would
/// become untypeable by mouse.
#[test]
fn a_panel_that_refuses_selection_does_not_refuse_it_for_a_field_inside_it() {
    let (mut nested, field) = field_in("quiet", "");
    press(&mut nested);
    assert!(field.selection().is_some_and(|range| range.start > 0));
}
