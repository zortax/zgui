//! A declared read that does not exist.

extern crate zgui_shader as zgui;

use zgui_shader::{NoParams, ShaderEffect, shader};

static WRONG: ShaderEffect<NoParams> = shader! {
    name: "wrong",
    mode: Paint,
    reads: [Backdrop],
    source: "",
};

fn main() {
    let _ = &WRONG;
}
