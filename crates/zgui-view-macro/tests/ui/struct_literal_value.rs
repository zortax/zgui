//! A struct literal written as a value, whose braces a value does not reach into.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(attrs)] attrs: Attrs) -> impl IntoView {
    let _ = attrs;
    ""
}

fn main() {
    let _ = view! { Thing(at = Point { x: 1, y: 2 }) };
}
