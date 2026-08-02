//! A comparison in an attribute value, which the tag spelling could not read.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(into)] flag: bool) -> impl IntoView {
    let _ = flag;
    ""
}

fn main() {
    let (a, b) = (1, 2);
    let _ = view! { Thing(flag = a > b) };
}
