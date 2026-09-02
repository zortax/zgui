//! A shader that compiles and declares nothing the mode calls.

extern crate zgui_shader as zgui;

use zgui_shader::{NoParams, ShaderEffect, shader};

static NOTHING: ShaderEffect<NoParams> = shader! {
    name: "nothing",
    mode: Paint,
    source: r#"
        fn helper(x: f32) -> f32 {
            return x;
        }
    "#,
};

fn main() {
    let _ = &NOTHING;
}
