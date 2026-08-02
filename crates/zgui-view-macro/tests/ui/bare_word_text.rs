//! Text is a string literal, because a bare word makes every expression ambiguous.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Holder(children: Children) -> impl IntoView {
    children.into_view_once()
}

fn main() {
    let _ = view! { Holder { hello } };
}
