//! An open popover whose content is replaced in place, and whose surface is restyled with it.
//!
//! The surface's entrance gives it a coordinate system of its own. The taller form flips the
//! surface above its anchor, and the same swap restyles the surface, which then gives that system
//! back. The header beside the form stays as it was. The frames after the swap draw the header
//! again, and they have to draw it in a system that still exists.

mod desktop;

use core::time::Duration;

use zgui::prelude::*;
use zgui::view::AnyView;
use zgui::{component, view};
use zgui_ui::overlay::{AnchoredSurfaceProps, OverlayState};
use zgui_ui::prelude::*;
use zgui_ui_primitives::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::desktop::stage::Stage;

/// The page: an empty anchor near the bottom, and a tone the surface's attribute selects.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }
                     .anchor { position: absolute; left: 80px; top: 640px; width: 0; height: 0 }
                     .tall { height: 220px }
                     .surface { width: 368px; padding: 0 }
                     [data-form=\"asking\"] { --tone: #a01010 }
                     [data-form=\"answered\"] { --tone: #1010a0 }
                     .head { color: var(--tone) }";

/// Long enough for the entrance to finish and for the frames after a swap to run.
const SETTLED: Duration = Duration::from_millis(600);

/// The first form: words, a toggle group and a key that swaps to the second.
#[component]
fn Asking(swap: RwSignal<bool, LocalStorage>) -> impl IntoView {
    view! {
        column {
            text {"Scale the deployment web in default"}
            ToggleGroup(selection = ToggleSelection::Single, label = "Replicas") {
                ToggleGroupItem(value = "1", label = "One") {"1"}
                ToggleGroupItem(value = "2", label = "Two") {"2"}
                ToggleGroupItem(value = "3", label = "Three") {"3"}
            }
            Button(on:click = move |_| swap.set(true)) {"Swap"}
        }
    }
}

/// The second form: wrapping words, room that flips the surface, and a row of keys.
#[component]
fn Answered(swap: RwSignal<bool, LocalStorage>) -> impl IntoView {
    view! {
        column {
            text {"The deployment is scaled to the replicas chosen, and the change is recorded."}
            box(class = "tall")
            row {
                Button(variant = ButtonVariant::Outline) {"Cancel"}
                Button(on:click = move |_| swap.set(false)) {"Back"}
            }
        }
    }
}

/// A popover on an empty anchor: a header that stays, and a form that follows `swap`.
#[component]
fn Swapping() -> impl IntoView {
    let swap = RwSignal::new_local(false);
    let state = OverlayState::uncontrolled(true, None);
    let form = move || Some(if swap.get() { "answered" } else { "asking" }.to_owned());
    view! {
        column(class = "page") {
            box(class = "anchor", node_ref = {state.trigger()})
            AnchoredSurface(
                state = state,
                placement = Placement::new(Side::Bottom, Align::Start),
                role = Role::Dialog,
                trap = FocusTrapOptions::MODAL,
                class = "surface",
                attr:data-form = form
            ) {
                text(class = "head") {"prod / default"}
                column {
                    {move || {
                        if swap.get() {
                            AnyView::new(view! { Answered(swap = swap) })
                        } else {
                            AnyView::new(view! { Asking(swap = swap) })
                        }
                    }}
                }
            }
        }
    }
}

#[test]
fn a_popover_whose_content_is_replaced_in_place_paints_on() {
    let mut stage = Stage::open(SHEET, || view! { ThemeProvider { Swapping() } });
    stage.hold(SETTLED);
    assert!(stage.shows("Swap"), "the first form is open");

    for _ in 0..2 {
        stage.click_saying("Swap");
        stage.hold(SETTLED);
        assert!(stage.shows("Back"), "the second form replaced the first");
        stage.click_saying("Back");
        stage.hold(SETTLED);
        assert!(stage.shows("Swap"), "the first form came back");
    }
}
