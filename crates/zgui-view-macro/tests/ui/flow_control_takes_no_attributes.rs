//! An attribute list written on control flow, which has nowhere to put one.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::view;

fn main() {
    let items: RwSignal<Vec<usize>> = RwSignal::new(Vec::new());
    let _ = view! {
        for item in move || items.get(), key = |item: &usize| *item, (class = "x") { {*item} }
    };
}
