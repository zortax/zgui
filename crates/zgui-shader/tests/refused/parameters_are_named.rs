//! Shader parameters compared by name must have names.

extern crate zgui_shader as zgui;

use zgui_shader::ShaderParams;

#[repr(C)]
#[derive(Clone, Copy, ShaderParams)]
struct Tuple(f32, f32);

fn main() {}
