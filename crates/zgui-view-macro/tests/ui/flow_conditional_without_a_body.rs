//! A conditional with no body, so nothing says what holding the condition shows.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::view;

fn main() {
    let open: RwSignal<bool> = RwSignal::new(false);
    let _ = view! { if move || open.get() { } };
}
