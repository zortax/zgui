//! A lower-case name in a view names an element, so a component's name is upper camel case.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::component;

#[component]
fn thing(#[prop(into)] label: String) -> impl IntoView {
    label
}

fn main() {}
