//! Drawing an element with a shader of the application's own.
//!
//! An effect is declared once, with [`shader!`](macro@shader), and compiled while the application
//! is built. Registering it hands back a [`ShaderHandle`], which carries the effect's parameters
//! and is what a custom element paints through.
//!
//! ```no_run
//! # extern crate zgui_shader as zgui;
//! use zgui_shader::{ShaderEffect, ShaderParams, shader};
//!
//! #[repr(C)]
//! #[derive(Clone, Copy, Default, ShaderParams)]
//! struct Trail {
//!     head: [f32; 2],
//!     length: f32,
//!     hue: f32,
//! }
//!
//! static TRAIL: ShaderEffect<Trail> = shader! {
//!     name: "cursor-trail",
//!     mode: Paint,
//!     params: Trail,
//!     reads: [Time],
//!     source: r#"
//!         struct Params {
//!             head: vec2<f32>,
//!             length: f32,
//!             hue: f32,
//!         }
//!
//!         fn shade(in: ShaderInput, params: Params) -> vec4<f32> {
//!             let reach = max(params.length, 1.0);
//!             let alpha = exp(-distance(in.local, params.head) / reach);
//!             return premultiplied(hsl(params.hue, 0.8, 0.6), alpha);
//!         }
//!     "#,
//! };
//!
//! let trail = TRAIL.register();
//! trail.set_params(Trail { head: [12.0, 8.0], length: 40.0, hue: 0.6 });
//! ```
//!
//! # What an effect costs
//!
//! An effect that declares no [reads](ShaderReads) costs what a background costs: it is one
//! instance in the same arena every quad lives in, drawn in the same pass, through the same clip,
//! and it repaints only when its parameters or its element change.
//!
//! An effect that declares `Time` repaints every refresh for as long as it is on screen, because
//! it draws something different every frame and nothing else knows that. That is the whole reason
//! the declaration exists.
//!
//! # The root the expansion names
//!
//! A `shader!` expansion names its crates through `zgui::shader`, so an application depends on the
//! umbrella crate alone. A crate with no umbrella over it supplies the root itself:
//!
//! ```
//! extern crate zgui_shader as zgui;
//! # fn main() {}
//! ```
//!
//! # Where an effect can be drawn
//!
//! Through [`ScenePainter::shade`](zgui_paint::content::custom::ScenePainter::shade), from a
//! custom element's `paint`. Everything the framework applies to a background — the clip chain,
//! the transform, the alpha folded in from the groups above — is applied to an effect too, so an
//! effect cannot draw outside the box it was given.

#![forbid(unsafe_code)]

// The root a `shader!` expansion names its crates through, so an application that declares an
// effect depends on the umbrella crate alone. This line is what makes the same paths resolve
// inside this crate, where there is no umbrella above it.
extern crate self as zgui;

/// This crate, under the name the expansion reaches it by.
pub use crate as shader;

mod effect;
mod handle;
mod paint;

pub use crate::effect::ShaderEffect;
pub use crate::handle::{NoParams, ParamsValue, ShaderHandle, ShaderParams};
pub use crate::paint::ShaderPainterExt;

pub use zgui_render_wgpu::{EffectProgram, ParamsField, ParamsLayout};
pub use zgui_scene::{ShaderId, ShaderParams as ShaderParamsBlock};
// The derive and the trait share a name deliberately, the way `Debug` does: one is a macro and the
// other a type, and an application writes `#[derive(ShaderParams)]` against the trait it names.
pub use zgui_shader_macro::{ShaderParams, shader};
pub use zgui_wgsl::{MAX_PARAMS_BYTES, ShaderMode, ShaderReads};
