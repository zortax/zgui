//! A listener written on a slot, which is a props struct and not an element.

extern crate zgui_view as zgui;

use zgui_view::prelude::*;
use zgui_view_macro::{component, slot, view};

/// A heading.
#[slot]
struct CardHeader {
    /// What it shows.
    children: Children,
}

/// A card that was never told it takes slots.
#[component]
fn Card(children: Children) -> impl IntoView {
    children.into_view_once()
}

fn main() {
    let _ = view! {
        Card {
            CardHeader(slot, on:click = |_| {}) { "Total" }
        }
    };
}
