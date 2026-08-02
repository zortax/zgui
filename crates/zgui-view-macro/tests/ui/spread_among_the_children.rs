//! A bundle of attributes written in the children block, one delimiter from its home.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(attrs)] attrs: Attrs) -> impl IntoView {
    let _ = attrs;
    ""
}

fn main() {
    let attrs = Attrs::new();
    let _ = view! { Thing(class = "c") {..attrs} };
}
