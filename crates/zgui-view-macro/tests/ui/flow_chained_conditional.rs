//! One conditional chained onto another.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::view;

fn main() {
    let open: RwSignal<bool> = RwSignal::new(false);
    let shut: RwSignal<bool> = RwSignal::new(false);
    let _ = view! {
        if move || open.get() { "open" } else if move || shut.get() { "shut" } else { "neither" }
    };
}
