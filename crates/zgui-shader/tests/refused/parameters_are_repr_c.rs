//! Shader parameters compared field by field must have a layout that promises field order.

extern crate zgui_shader as zgui;

use zgui_shader::ShaderParams;

#[derive(Clone, Copy, ShaderParams)]
struct Loose {
    amount: f32,
}

fn main() {}
