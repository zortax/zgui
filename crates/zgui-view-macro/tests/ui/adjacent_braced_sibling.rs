//! A childless call followed by a braced sibling, which writes its empty block to keep them
//! apart.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(optional)] children: Option<Children>) -> impl IntoView {
    children.map(Children::into_view_once)
}

fn main() {
    let ticks = || "x";
    let _ = view! { Thing() {} {move || ticks()} };
}
