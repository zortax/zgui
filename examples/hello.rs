//! The smallest application: one line of text.
//!
//! Run it with `cargo run -p zgui-examples --example hello`.

use zgui::prelude::*;

fn main() -> Result<(), zgui::Error> {
    app()
        .with_title("Hello")
        .with_size(480.0, 320.0)
        .with_stylesheet(":root { background: white; } .hello { color: black; font-size: 24px; }")
        .run(|| view! { text(class = "hello") {"Hello"} })
}
