//! A collection read once, where a list that re-reads it belongs.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::view;

fn main() {
    let items: RwSignal<Vec<usize>> = RwSignal::new(Vec::new());
    let _ = view! { for item in items.get(), key = |item: &usize| *item { {*item} } };
}
