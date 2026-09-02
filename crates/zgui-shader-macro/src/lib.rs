//! The macros an application declares a shader with.
//!
//! Both of them exist so that a mistake is a build failure rather than a rectangle full of the
//! wrong pixels. [`shader!`](macro@shader) assembles the whole translation unit and compiles it
//! while the application is built, so WGSL that does not parse fails the build with the shader
//! front end's own message. [`ShaderParams`](derive@ShaderParams) reads the parameter structure's
//! layout out of the compiler, so the comparison against what the shader declares is against what
//! Rust actually chose rather than against what someone wrote down.

#![forbid(unsafe_code)]

mod effect;
mod params;

use proc_macro::TokenStream;

/// Declares an application shader, compiling it while the application is built.
///
/// ```ignore
/// use zgui::prelude::*;
/// use zgui::shader::{ShaderEffect, ShaderParams, shader};
///
/// #[repr(C)]
/// #[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable, ShaderParams)]
/// struct Trail {
///     head: [f32; 2],
///     length: f32,
///     hue: f32,
/// }
///
/// static TRAIL: ShaderEffect<Trail> = shader! {
///     name: "cursor-trail",
///     mode: Paint,
///     params: Trail,
///     reads: [Time],
///     source: r#"
///         struct Params {
///             head: vec2<f32>,
///             length: f32,
///             hue: f32,
///         }
///
///         fn shade(in: ShaderInput, params: Params) -> vec4<f32> {
///             let distance = distance(in.local, params.head);
///             let alpha = exp(-distance / max(params.length, 1.0));
///             return premultiplied(hsl(params.hue, 0.8, 0.6), alpha);
///         }
///     "#,
/// };
/// ```
///
/// # Fields
///
/// `name` labels the effect, and is what CSS names it by. `mode` is `Paint` or `Coverage`.
/// `params` is the Rust structure the shader's own `Params` must agree with. `reads` lists what
/// the effect reads that changes on its own — `Time`, `Pointer` — and is optional; declaring
/// nothing is an effect that repaints only when its parameters or its style change. Exactly one of
/// `source` and `path` gives the text.
#[proc_macro]
pub fn shader(input: TokenStream) -> TokenStream {
    effect::expand(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Reads a parameter structure's layout out of the compiler.
///
/// The structure must be `#[repr(C)]`, because the layout it is compared against is a shader's,
/// and the default representation promises nothing about field order.
#[proc_macro_derive(ShaderParams)]
pub fn shader_params(input: TokenStream) -> TokenStream {
    params::expand(input.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
