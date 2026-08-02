//! A prop the component does not have.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(into)] label: String) -> impl IntoView {
    label
}

fn main() {
    let _ = view! { Thing(label = "a", colour = "red") };
}
