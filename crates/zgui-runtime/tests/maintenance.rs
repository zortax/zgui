//! Idle resource maintenance is a wake, not a frame.

mod support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use zgui_runtime::embed::{EmbedHost, EmbedMaintenanceCx, EmbedSyncCx, EmbedSyncReport};
use zgui_view::{BuildCx, IntoView, View};

struct ObservedMaintenance(Rc<Cell<u32>>);

impl EmbedHost for ObservedMaintenance {
    fn sync(&mut self, _cx: &mut EmbedSyncCx<'_>) -> EmbedSyncReport {
        EmbedSyncReport::default()
    }

    fn maintain(&mut self, _cx: &mut EmbedMaintenanceCx<'_>) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn a_maintenance_deadline_trims_without_painting_or_presenting() {
    let maintained = Rc::new(Cell::new(0));
    let mut harness = support::app(
        "root { display: block; width: 400px; height: 300px }",
        |cx: &mut BuildCx<'_>| Box::new(zgui_elements::r#box().into_view().build(cx)),
    );
    harness.app_mut().windows_mut()[0]
        .install_embed_host(Box::new(ObservedMaintenance(Rc::clone(&maintained))));
    harness.settle(8);
    harness.reset_counts();

    harness.advance(Duration::from_secs(2));
    assert_eq!(maintained.get(), 1, "the idle maintenance hook ran once");
    assert_eq!(harness.pump(), 0, "maintenance did not build a frame");
    assert_eq!(
        harness.frames_requested(),
        0,
        "maintenance requested no redraw"
    );

    assert_eq!(
        harness.run_for(Duration::from_secs(10), Duration::from_secs(1)),
        0,
        "a parked static document did no continuing CPU or frame work"
    );
    assert_eq!(maintained.get(), 1, "maintenance did not re-arm itself");
}

#[test]
fn a_maintenance_wake_notes_every_recorded_chunk_for_the_next_frame() {
    // The renderer gives its resident chunks back when the window goes idle, and a chunk is made
    // resident again only from a note. A painting that never encodes again is otherwise served
    // afresh by every later frame that draws it.
    let mut harness = support::app(
        "root { display: block; width: 400px; height: 300px }
         box { display: block; width: 100px; height: 40px; background-color: rgb(9, 90, 9) }",
        |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(zgui_elements::r#box())
                    .child(zgui_elements::r#box())
                    .into_view()
                    .build(cx),
            )
        },
    );
    harness.settle(8);
    let before = harness.app().windows()[0].scene().chunk_inserted().len();

    harness.advance(Duration::from_secs(2));
    let after = harness.app().windows()[0].scene().chunk_inserted().len();
    assert!(
        after > before,
        "the idle release left the chunks un-noted: {before} notes before it, {after} after"
    );
}
