//! A bare name written as a condition, which may already hold the closure it needs.

extern crate zgui_view as zgui;

use zgui_view_macro::view;

fn main() {
    let chosen = move || true;
    let _ = view! { if chosen { "shown" } };
}
