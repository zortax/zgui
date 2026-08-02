//! A braced attribute that does not spread a bundle.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(attrs)] attrs: Attrs) -> impl IntoView {
    let _ = attrs;
    ""
}

fn main() {
    let forwarded = Attrs::new();
    let _ = view! { Thing({forwarded}) };
}
