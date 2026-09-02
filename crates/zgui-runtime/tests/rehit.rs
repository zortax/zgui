//! A stationary pointer is re-tested only when what is under it could have changed.
//!
//! A surface that opens under the cursor must become hovered without the pointer moving, which
//! is why the frame re-tests under a still pointer at all. But a frame in which no fragment
//! appeared, moved or went has left the pointer over exactly what it was over, and re-testing it
//! is a hit query and a hover diff paid for nothing — on every frame of an animation.

mod support;

use std::rc::Rc;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_profile::{Counter, counter};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::*;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .cover { display: block; width: 400px; height: 300px }
                   .cover.tinted { background: #123456 }
                   .late { display: block; width: 400px; height: 300px }";

fn park_pointer(app: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>) {
    app.deliver_to_first(SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(Point::new(CssPx(200.0), CssPx(150.0))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    app.settle(4);
}

#[test]
fn a_frame_that_moved_no_fragment_does_not_retest_under_the_pointer() {
    zgui_reactive::install().ok();
    let tinted = RwSignal::new(false);
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("cover")
                .class_toggle(zgui_interned::ClassName::new("tinted"), move || {
                    tinted.get()
                })
                .into_view()
                .build(cx),
        )
    });
    app.settle(4);
    park_pointer(&mut app);

    let _guard = counter::exclusive();
    counter::reset();
    tinted.set(true);
    app.settle(4);
    assert_eq!(
        counter::snapshot().get(Counter::HitRetests),
        0,
        "a colour change moved nothing under the pointer, so nothing was re-tested"
    );
    app.shut_down();
}

#[test]
fn a_box_that_appears_under_the_pointer_becomes_hovered_without_a_move() {
    zgui_reactive::install().ok();
    let shown = RwSignal::new(false);
    let hovered = Rc::new(std::cell::Cell::new(false));
    let seen = Rc::clone(&hovered);
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let seen = Rc::clone(&seen);
        Box::new(
            zgui_elements::column()
                .class("cover")
                .child(zgui_view::Show::new(
                    move || shown.get(),
                    move || {
                        let seen = Rc::clone(&seen);
                        zgui_view::AnyView::new(
                            zgui_elements::column()
                                .class("late")
                                .on(zgui_view::events::POINTER_ENTER, move |_| seen.set(true)),
                        )
                    },
                ))
                .into_view()
                .build(cx),
        )
    });
    app.settle(4);
    park_pointer(&mut app);

    let _guard = counter::exclusive();
    counter::reset();
    shown.set(true);
    app.settle(4);
    assert!(
        counter::snapshot().get(Counter::HitRetests) >= 1,
        "a box appeared under the pointer, so the frame re-tested"
    );
    assert!(
        hovered.get(),
        "the box that opened under the still pointer was entered"
    );
    app.shut_down();
}
