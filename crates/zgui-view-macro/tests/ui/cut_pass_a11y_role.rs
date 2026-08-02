//! An unbraced path in an `a11y:` value, which is one of the nine forms written with an
//! expression: it must keep parsing exactly as the cut rule says.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(attrs)] attrs: Attrs) -> impl IntoView {
    let _ = attrs;
    ""
}

fn main() {
    let _ = view! { Thing(a11y:role = zgui_view::Role::Button, a11y:label = "Save") };
}
