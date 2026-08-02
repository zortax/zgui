//! The same comparison inside a closure.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(into)] test: bool) -> impl IntoView {
    let _ = test;
    ""
}

fn main() {
    let _ = view! { Thing(test = (|x: u8| x > 1)(2)) };
}
