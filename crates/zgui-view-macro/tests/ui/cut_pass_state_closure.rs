//! An unbraced closure in a `state:` value, which is another of the nine.

extern crate zgui_view as zgui;

use zgui_reactive::{RwSignal, install};
use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(attrs)] attrs: Attrs) -> impl IntoView {
    let _ = attrs;
    ""
}

fn main() {
    install().ok();
    let flag = RwSignal::new(false);
    let _ = view! { Thing(state:disabled = move || flag.get()) };
}
