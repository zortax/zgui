//! A Rust range written where a list names the collection it rebuilds itself from.

extern crate zgui_view as zgui;

use zgui_view_macro::view;

fn main() {
    let _ = view! { for index in 0..10, key = |index: &usize| *index { {*index} } };
}
