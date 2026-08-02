//! A pattern match written as a condition.

extern crate zgui_view as zgui;

use zgui_view_macro::view;

fn main() {
    let label: Option<&str> = None;
    let _ = view! { if let Some(text) = label { {text} } };
}
