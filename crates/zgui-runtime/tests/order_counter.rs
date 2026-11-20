//! The counter of order entries a window writes into its compositor.
//!
//! The counters are a process-wide block, so this is one test in a target of its own.

mod support;

use zgui_profile::{COUNTERS_ENABLED, Counter};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_testkit_scene::counters::Recording;
use zgui_view::{BuildCx, ClassName, IntoView, View};

/// A column of swatches, one of which can be lit.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .swatch { display: block; width: 40px; height: 20px; background-color: #303030 }
                   .lit { background-color: #c0c0c0 }";

#[test]
fn a_mounted_child_writes_order_entries_and_a_recolour_writes_none() {
    let mut recording = Recording::begin();
    let (lit, extra) = (RwSignal::new(false), RwSignal::new(false));
    let mut harness = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::column()
                        .class("swatch")
                        .class_toggle(ClassName::new("lit"), move || lit.get()),
                )
                .child(zgui_view::Show::new(
                    move || extra.get(),
                    || zgui_view::AnyView::new(zgui_elements::column().class("swatch")),
                ))
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);

    let recoloured = recording.measure(|| {
        lit.set(true);
        harness.settle(8);
    });
    let mounted = recording.measure(|| {
        extra.set(true);
        harness.settle(8);
    });
    if !COUNTERS_ENABLED {
        return;
    }

    assert!(
        recoloured.get(Counter::ChunksReencoded) > 0,
        "the recolour recorded nothing, so the zero below measures nothing"
    );
    assert_eq!(
        recoloured.get(Counter::OrderEntriesDiffed),
        0,
        "a recolour changes no list of the paint order"
    );
    assert!(
        mounted.get(Counter::OrderEntriesDiffed) > 0,
        "a child mounted into the column wrote no order entry"
    );
}
