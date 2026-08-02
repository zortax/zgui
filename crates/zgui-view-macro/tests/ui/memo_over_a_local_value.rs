//! A memo is shared between threads, so what it holds must be able to be.

extern crate zgui_view as zgui;

use std::rc::Rc;

use zgui_reactive::{Memo, install};
use zgui_view::prelude::*;
use zgui_view_macro::{component, view};

#[component]
fn Thing(#[prop(into)] label: String) -> impl IntoView {
    label
}

fn main() {
    install().ok();
    let shared = Memo::new(move |_| Rc::new(String::from("a")));
    let _ = view! { Thing(label = move || shared.get().to_string()) };
}
