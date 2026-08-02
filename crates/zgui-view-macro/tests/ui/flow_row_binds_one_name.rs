//! A row bound to a pattern, where a list binds one name.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::view;

fn main() {
    let pairs: RwSignal<Vec<(usize, usize)>> = RwSignal::new(Vec::new());
    let _ = view! {
        for (at, gap) in move || pairs.get(), key = |pair: &(usize, usize)| pair.0 { {gap} }
    };
}
