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
