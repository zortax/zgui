//! A generic argument list written without a turbofish, which is not an expression.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(into)] items: Vec<u8>) -> impl IntoView {
    let _ = items;
    ""
}

fn main() {
    let _ = view! { Thing(items = Vec<u8>::new()) };
}
