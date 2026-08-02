//! A scroll offset is state, and state does not evaporate because the document changed shape.
//!
//! Everything a real interface does — a tooltip appearing, a menu opening, a control revealed on
//! hover, a row added by a handler — adds or removes a box somewhere. Adding or removing a box
//! anywhere in the document rebuilds the *whole* box tree, and every box in it is issued a new key.
//! So anything filed under a box is filed under a name nothing will ask about again the moment
//! anything at all changes.
//!
//! That is what these assert against, and it is worth being precise about why it is so hard to see.
//! Nothing fails. The scroll was clamped correctly, the container was marked correctly, the
//! fragments were composed correctly against the offset the scroller was asked for — which is zero,
//! because the offset that was written is filed elsewhere. The page is simply at the top again, and
//! every counter, every damage rectangle and every invariant in the frame agrees that it should be.
//!
//! The offsets are read out of the scroller directly rather than off the picture, because a page
//! that scrolled and silently returned to the top looks exactly like a page that never scrolled.

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_vocab::{
    Modifiers, PointerAction, PointerEvent, ScrollDelta, ScrollPhase, Timestamp, WheelEvent,
};

/// One frame at the surface's refresh rate, rounded up past the deadline the park installs.
const FRAME: Duration = Duration::from_millis(20);

/// A scrolling list, a rule that fires on hover, and a box that appears elsewhere on demand.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow: scroll; line-height: 20px }
.row { display: block; width: 400px; height: 20px; background-color: #202020 }
.row:hover { background-color: #ff0000 }
.extra { display: block; width: 10px; height: 10px }
";

/// The application under test.
type Window = zgui_platform_headless::Harness<zgui_runtime::Runtime>;

/// The way back to the signal a view created while it was being built.
///
/// A view is built by a closure the harness owns, so a test that wants to write to a signal the
/// view made has to be handed it from inside that closure rather than making one first.
type Handed = Rc<RefCell<Option<RwSignal<bool, zgui_reactive::LocalStorage>>>>;

/// A window holding the list, and the signal that makes a box appear somewhere else in the page.
fn listing() -> (Window, Handed) {
    let slot: Handed = Rc::new(RefCell::new(None));
    let handed = Rc::clone(&slot);
    let harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        use zgui_view::{IntoView, View};
        let extra = RwSignal::new_local(false);
        *handed.borrow_mut() = Some(extra);
        let mut port = zgui_elements::column().class("port");
        for _ in 0..200 {
            port = port.child(zgui_elements::column().class("row"));
        }
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(port)
                .child(zgui_view::Show::new(
                    move || extra.get(),
                    || zgui_view::AnyView::new(zgui_elements::column().class("extra")),
                ))
                .into_view()
                .build(cx),
        )
    });
    (harness, slot)
}

/// The window the harness is driving.
fn window(harness: &Window) -> &zgui_runtime::Window {
    harness.app().windows().first().expect("a window")
}

/// Every offset the scroller is holding for a container that is still in the document.
///
/// A rekeyed container shows up here as a *loss* rather than as a wrong number, which is why this
/// is a list of what is still reachable rather than a reading of one container: the offset does not
/// change when this defect bites, it stops being findable.
fn offsets(harness: &Window) -> Vec<f32> {
    let window = window(harness);
    let layout = window.layout().borrow();
    let scroll = window.scroll().borrow();
    layout
        .keys()
        .into_iter()
        .filter_map(|key| layout.node(key).source)
        .map(|element| scroll.offset_of(element).y.0)
        .filter(|offset| *offset != 0.0)
        .collect()
}

/// Scrolls the list well away from the top and lets the motion finish.
fn scrolled() -> (Window, Handed, Vec<f32>) {
    let (mut harness, slot) = listing();
    harness.settle(8);
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(60.0)),
            delta: ScrollDelta::Lines { x: 0.0, y: 5.0 },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    for _ in 0..30 {
        harness.advance(FRAME);
        harness.pump();
    }
    harness.settle(4);
    let at = offsets(&harness);
    assert!(
        !at.is_empty(),
        "the wheel moved nothing, so nothing is being asserted"
    );
    (harness, slot, at)
}

#[test]
fn a_box_appearing_elsewhere_in_the_document_does_not_move_the_list() {
    let (mut harness, slot, before) = scrolled();

    // A tooltip, a dropdown, a hover-revealed control and a row added by a handler are all this.
    slot.borrow()
        .expect("the view handed its signal over")
        .set(true);
    harness.settle(8);

    assert_eq!(
        offsets(&harness),
        before,
        "a box appearing somewhere else in the page sent the list back to the top"
    );
}

#[test]
fn a_box_disappearing_again_does_not_move_the_list_either() {
    let (mut harness, slot, before) = scrolled();
    let signal = slot.borrow().expect("the view handed its signal over");
    signal.set(true);
    harness.settle(8);
    signal.set(false);
    harness.settle(8);

    assert_eq!(
        offsets(&harness),
        before,
        "closing what had opened sent the list back to the top"
    );
}

#[test]
fn hovering_a_row_does_not_move_the_list() {
    let (mut harness, _slot, before) = scrolled();

    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(Point::new(CssPx(200.0), CssPx(70.0))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(8);

    assert_eq!(
        offsets(&harness),
        before,
        "moving the pointer over a row that restyles on hover moved the list"
    );
}

#[test]
fn the_list_is_still_where_it_was_after_everything_at_once() {
    // The combination, because these arrive together in a real interface: a pointer lands on
    // something, its hover rule fires, and what it reveals adds a box.
    let (mut harness, slot, before) = scrolled();
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(Point::new(CssPx(200.0), CssPx(70.0))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    slot.borrow()
        .expect("the view handed its signal over")
        .set(true);
    harness.settle(8);
    for _ in 0..10 {
        harness.advance(FRAME);
        harness.pump();
    }

    assert_eq!(
        offsets(&harness),
        before,
        "the list did not stay where it was"
    );
}
