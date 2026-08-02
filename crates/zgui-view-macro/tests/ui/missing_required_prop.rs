//! A prop with no default, left out.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(
    /// What it says.
    #[prop(into)]
    label: String,
    /// How many times it says it.
    #[prop(default = 1)]
    times: usize,
) -> impl IntoView {
    label.repeat(times)
}

fn main() {
    let _ = view! { Thing(times = 2) };
}
