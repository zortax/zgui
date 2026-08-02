//! A word control flow uses, written as the name of a node.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Holder(children: Children) -> impl IntoView {
    children.into_view_once()
}

fn main() {
    let _ = view! { Holder { while(open = flag) { "x" } } };
}
