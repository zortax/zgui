//! A modifier the listener grammar does not have.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(attrs)] attrs: Attrs) -> impl IntoView {
    let _ = attrs;
    ""
}

fn main() {
    let _ = view! { Thing(on:click:onc = |_| {}) };
}
