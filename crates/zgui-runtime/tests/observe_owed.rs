//! A geometry delivery that leaves work behind buys the frame that finishes it.
//!
//! Observed geometry is delivered inside the frame, and what its readers write is flushed and
//! laid out again there, up to a small number of passes. Two things can be left over when that
//! stops: reactive work the delivery's own flush could not finish, and geometry the last relayout
//! produced that nobody has been told about yet. Both were silent — the flush there runs with
//! its wakes folded into the frame, so its outcome is the only record — and a view that needs one
//! more pass sat unchanged until something unrelated asked for a frame.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_reactive::RwSignal;
use zgui_reactive::prelude::*;
use zgui_view::{BuildCx, IntoView, NodeRef, View};

const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .grows { display: block; width: 100px }";

/// The height a box climbs to, twenty pixels per delivery.
const TOP: f32 = 200.0;

#[test]
fn a_delivery_that_does_not_settle_in_its_passes_is_finished_by_the_next_frame() {
    zgui_reactive::install().ok();
    let height = RwSignal::new(100.0_f32);
    let seen: Rc<Cell<f32>> = Rc::new(Cell::new(0.0));
    let recorded = Rc::clone(&seen);
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let handle = NodeRef::new();
        let recorded = Rc::clone(&recorded);
        // Each delivery of the box's height asks for twenty pixels more, until the top. Every
        // step is one relayout and one delivery, so the climb takes more passes than one frame
        // is allowed.
        core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
            if handle.get().is_none() {
                return;
            }
            let box_of = handle.observe_border_box();
            let recorded = Rc::clone(&recorded);
            core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
                let Some(observed) = box_of.get() else {
                    return;
                };
                let measured = observed.size.height.0;
                recorded.set(measured);
                let next = (measured + 20.0).min(TOP);
                if next > height.get_untracked() {
                    height.set(next);
                }
            }));
        }));
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::column()
                        .class("grows")
                        .node_ref(handle)
                        .style_property("height", move || Some(format!("{}px", height.get()))),
                )
                .into_view()
                .build(cx),
        )
    });

    // Enough turns for the whole climb, and no input of any kind in between.
    app.settle(32);
    assert_eq!(
        seen.get(),
        TOP,
        "the box stopped climbing at {}px: a delivery that did not settle left the rest of the \
         climb to a frame nothing asked for",
        seen.get()
    );
    app.shut_down();
}
