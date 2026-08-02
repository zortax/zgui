//! A shift in an attribute value, which the tag spelling read as two closing tags.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(into)] mask: u8) -> impl IntoView {
    let _ = mask;
    ""
}

fn main() {
    let bits = 8_u8;
    let _ = view! { Thing(mask = bits >> 2) };
}
