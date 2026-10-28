//! A layout pass that services a rebreak retires it.
//!
//! A rebreak left set on the root makes every later delivery owe more work, so each scroll frame
//! restyles and lays out the document again.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// One frame of the surface's refresh rate, rounded up past the deadline the park installs.
const FRAME: std::time::Duration = std::time::Duration::from_millis(20);

#[test]
fn a_rebreak_does_not_make_later_deliveries_lay_out_again() {
    // A paragraph whose alignment flips, which owes a rebreak, and a probe inside a scrolled port
    // whose border box is observed. Its observer reads the box and writes nothing.
    let centred: Rc<Cell<Option<RwSignal<bool, zgui_reactive::LocalStorage>>>> =
        Rc::new(Cell::new(None));
    let handed = Rc::clone(&centred);
    let seen = Rc::new(Cell::new(0_u32));
    let read = Rc::clone(&seen);
    let mut harness = support::app_with_text(
        "root { display: block; width: 400px; height: 300px }
         .port { display: block; width: 400px; height: 120px; overflow-y: scroll }
         .row { display: block; width: 400px; height: 20px }
         .probe { display: block; width: 10px; height: 10px }
         .text { display: block; width: 200px }",
        move |cx: &mut BuildCx<'_>| {
            let flip = RwSignal::new_local(false);
            handed.set(Some(flip));
            let probe = NodeRef::new();
            let read = Rc::clone(&read);
            core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
                if probe.get().is_none() {
                    return;
                }
                let box_of = probe.observe_border_box();
                let read = Rc::clone(&read);
                core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
                    if box_of.get().is_some() {
                        read.set(read.get() + 1);
                    }
                }));
            }));
            let mut port = zgui_elements::column()
                .class("port")
                .child(zgui_elements::column().class("probe").node_ref(probe));
            for _ in 0..40 {
                port = port.child(zgui_elements::column().class("row"));
            }
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(
                        zgui_elements::text()
                            .class("text")
                            .style_property("text-align", move || {
                                Some(if flip.get() { "center" } else { "left" }.to_owned())
                            })
                            .child("a paragraph"),
                    )
                    .child(port)
                    .into_view()
                    .build(cx),
            )
        },
    );
    harness.settle(8);
    let flip = centred.get().expect("the view handed its signal over");
    flip.set(true);
    harness.settle(8);
    {
        let window = &harness.app().windows()[0];
        let document = window.document().borrow();
        let root = document.root_index().expect("a root");
        let (own, below) = document.store().core(root).dirty().get();
        assert!(
            !(own | below).contains(zgui_bits::Dirty::REBREAK),
            "the layout pass left the rebreak it serviced standing"
        );
    }

    let delivered = seen.get();
    let before = zgui_profile::counter::snapshot();
    harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(100.0)),
            delta: ScrollDelta::Lines { x: 0.0, y: 2.0 },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    let mut frames = 0;
    for _ in 0..12 {
        harness.advance(FRAME);
        frames += harness.pump();
    }
    let moved = before.delta(&zgui_profile::counter::snapshot());
    assert!(
        seen.get() > delivered + 2,
        "the probe's box was delivered {} times over the glide",
        seen.get() - delivered
    );
    assert!(frames > 2, "the glide ran {frames} frames");
    assert_eq!(
        moved.layout_passes, frames,
        "{frames} frames ran {} layout passes: a delivery that changed nothing laid out again",
        moved.layout_passes
    );
    harness.shut_down();
}
