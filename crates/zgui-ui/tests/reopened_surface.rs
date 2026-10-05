//! A surface that opens a second time builds its content over handles the first build bound.
//!
//! The handle lives in the component that owns the surface, so it outlives each build of the
//! content. The list inside observes its scroll position through that handle. Each new build
//! has to start from an unbound handle, or the observation names a node the first build took
//! away.

mod desktop;

use core::time::Duration;

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::{component, view};
use zgui_ui::overlay::{AnchoredSurfaceProps, OverlayState};
use zgui_ui::prelude::*;
use zgui_ui_primitives::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::desktop::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }
                     .surface { width: 240px; padding: 4px }
                     .rows { height: 120px }";

/// Long enough for an entrance or an exit to finish.
const SETTLED: Duration = Duration::from_millis(600);

/// A key that opens a surface holding a virtualised list and a key that closes it.
#[component]
fn Reopening() -> impl IntoView {
    let state = OverlayState::uncontrolled(false, None);
    let port = NodeRef::new();
    let rows = RwSignal::new_local(40_usize);
    view! {
        column(class = "page") {
            Button(node_ref = {state.trigger()}, on:click = move |_| state.open()) {"Open"}
            AnchoredSurface(
                state = state,
                placement = Placement::new(Side::Bottom, Align::Start),
                role = Role::Dialog,
                trap = FocusTrapOptions::MODAL,
                class = "surface"
            ) {
                VirtualList(
                    count = rows,
                    row_size = 20.0,
                    node_ref = port,
                    class = "rows",
                    row = move |index: usize| view! { text {{format!("row {index}")}} }
                )
                Button(on:click = move |_| state.close()) {"Close"}
            }
        }
    }
}

#[test]
fn a_surface_whose_list_handle_outlives_it_opens_again() {
    let mut stage = Stage::open(SHEET, || view! { ThemeProvider { Reopening() } });
    stage.hold(SETTLED);
    for round in 0..3 {
        stage.click_saying("Open");
        stage.hold(SETTLED);
        assert!(stage.shows("row 0"), "round {round}: the list is open");
        stage.click_saying("Close");
        stage.hold(SETTLED);
        assert!(!stage.shows("row 0"), "round {round}: the list is closed");
    }
}
