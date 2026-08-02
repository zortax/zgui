//! A braced expression child inside a children block, which is two braces and one node.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Holder(children: Children) -> impl IntoView {
    children.into_view_once()
}

fn main() {
    let label = "a";
    let _ = view! { Holder { {label} } };
}
