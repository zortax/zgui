//! What one frame does, and — more often the interesting question — what it declines to do.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_profile::{Counter, counter};
use zgui_view::{BuildCx, IntoView, NodeRef, View};

/// The sheet every fixture here is styled by.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block; width: 100px; height: 50px }
                   text { display: block }";

/// The counters are one block of atomics for the whole process, so a measurement and its control
/// run one after the other on one thread rather than as two tests the runner may interleave.
#[test]
fn nothing_observing_means_no_observation_pass_at_all() {
    // The ordinary document: nothing watches its own geometry, and the whole delivery stage is one
    // emptiness test rather than a walk over every node that asked for nothing.
    let mut plain = support::app(CSS, |cx: &mut BuildCx<'_>| {
        let mut view = zgui_elements::column().class("root");
        for _ in 0..50 {
            view = view.child(zgui_elements::column());
        }
        Box::new(view.into_view().build(cx))
    });
    counter::reset();
    plain.settle(8);
    let without = counter::get(Counter::ObservationPasses);
    plain.shut_down();
    drop(plain);

    // The control, and it is not optional: a counter nothing increments reads zero for every
    // document there is, and an emptiness test that is never false is not a fast path.
    let observed: Rc<Cell<Option<zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>>>> =
        Rc::new(Cell::new(None));
    let seen = Rc::clone(&observed);
    let mut watching = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let handle = NodeRef::new();
        let seen = Rc::clone(&seen);
        // Observation starts once the handle names a node, which is what an overlay positioned
        // against its anchor does. Held for the life of the window: a dropped effect stops.
        core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
            if handle.get().is_none() {
                return;
            }
            let box_of = handle.observe_border_box();
            let seen = Rc::clone(&seen);
            core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
                use zgui_reactive::prelude::Get;
                seen.set(box_of.get());
            }));
        }));
        Box::new(
            zgui_elements::column()
                .class("root")
                .node_ref(handle)
                .into_view()
                .build(cx),
        )
    });
    counter::reset();
    watching.settle(8);
    let with = counter::get(Counter::ObservationPasses);

    assert_eq!(
        without, 0,
        "a document with nothing observing paid for a delivery pass"
    );
    assert!(
        with >= 1,
        "one node observing its own box paid for no delivery pass at all, so the assertion above \
         holds for a stage that never runs"
    );
    let delivered = observed.get().expect("the observed box reached the view");
    assert!(
        delivered.size.width.0 > 0.0,
        "the value delivered was empty: {delivered:?}"
    );
}
