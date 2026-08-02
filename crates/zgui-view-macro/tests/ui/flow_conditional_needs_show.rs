//! A conditional written where the component it is sugar for is not in scope.

extern crate zgui_view as zgui;

use zgui_view_macro::view;

fn main() {
    let open = true;
    let _ = view! { if move || open { "shown" } };
}
