//! A virtualised list the compositor scrolls wakes the window only for the rows it has to build.

mod desktop;

use zgui::geom::{DevicePx, Point};
use zgui::reactive::RwSignal;
use zgui::view;
use zgui_profile::Counter;
use zgui_ui::prelude::*;

use crate::desktop::stage::Stage;

const SHEET: &str = ".list { width: 300px; height: 200px }
                     .line { height: 20px; background-color: #202020 }";

#[test]
fn a_wheel_glide_over_a_virtual_list_wakes_the_window_only_to_recycle_rows() {
    let mut stage = Stage::open(SHEET, || {
        let count = RwSignal::new_local(10_000_usize);
        view! {
            VirtualList(
                count = count,
                row_size = 20.0,
                class = "list",
                row = move |index: usize| view! {
                    box(class = "line") { text {{move || format!("row {index}")}} }
                }
            )
        }
    });
    stage.move_to(Point::new(DevicePx(150.0), DevicePx(100.0)));
    assert!(!stage.shows("row 15"), "the list starts at its first rows");

    let before = zgui_profile::counter::snapshot();
    stage.wheel(2.0);
    let moved = before.delta(&zgui_profile::counter::snapshot());

    assert!(
        stage.shows("row 15"),
        "the glide did not bring row 15 into view"
    );
    assert!(
        moved.get(Counter::BoxesRebuilt) > 0,
        "the glide recycled no row"
    );
    assert!(
        moved.get(Counter::CompositorTicks) > moved.get(Counter::ZguiFrames),
        "the window ran {} frames against {} of the compositor's, so it ran one per frame",
        moved.get(Counter::ZguiFrames),
        moved.get(Counter::CompositorTicks)
    );
}
