//! A condition read once, where one that is re-read belongs.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::view;

fn main() {
    let open: RwSignal<bool> = RwSignal::new(false);
    let _ = view! { if open.get() { "shown" } };
}
