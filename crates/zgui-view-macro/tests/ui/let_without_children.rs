//! An argument named for children that are not there.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Holder(children: Children) -> impl IntoView {
    children.into_view_once()
}

fn main() {
    let _ = view! { Holder(let:item) };
}
