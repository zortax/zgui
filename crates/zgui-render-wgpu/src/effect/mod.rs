//! Application shaders: what one is, how it is compiled, and the pipelines built from it.
//!
//! An effect reaches this crate as a [`EffectProgram`] — a representation already lowered by the
//! application's own build, and the text it was lowered from. The registry keys them by
//! [`ShaderId`], and everything above the render seam names nothing but that handle.
//!
//! # Why the text travels beside the representation
//!
//! The framework's own modules are decoded with no fallback: they were encoded by the build that
//! is decoding them, so a decode failure means this crate's build output was replaced and there is
//! nothing better to do than say so. An application's effect is different. It was encoded by the
//! application's build, and the shader front end that wrote it is a dependency the application
//! resolves for itself, so the two ends can differ by a version. Falling back to the text costs
//! one parse and never fails to draw, which is the right trade for a mismatch that a lock file
//! could reintroduce at any time.

mod declared;
mod registry;

pub use crate::effect::declared::{count as declared_count, declare, since as declared_since};
pub use crate::effect::registry::{Effects, check_params};

use zgui_wgsl::ShaderMode;

/// One application effect, as its own build left it.
///
/// Both halves are `'static` because both are emitted by the `shader!` macro into the
/// application's binary. Nothing here is allocated at run time.
#[derive(Clone, Copy, Debug)]
pub struct EffectProgram {
    /// What the effect does, which decides the pipeline's entry points.
    pub mode: ShaderMode,
    /// A label, so a driver error names which effect produced it.
    pub label: &'static str,
    /// The whole translation unit, already parsed and lowered by the application's build.
    pub representation: &'static [u8],
    /// The text that representation was lowered from.
    pub source: &'static str,
    /// The layout of the effect's `Params` structure, as Rust has it.
    pub params: ParamsLayout,
}

/// What Rust says an effect's parameters are, so the shader's declaration can be checked.
///
/// A field added on the Rust side alone shifts every field after it on that side alone, which is
/// a wrong picture with no error attached to it. This is the comparison that turns that into a
/// failure at registration.
#[derive(Clone, Copy, Debug)]
pub struct ParamsLayout {
    /// How many bytes the structure occupies.
    pub size: usize,
    /// Its fields, in declaration order.
    pub fields: &'static [ParamsField],
}

/// One field of an effect's parameters.
///
/// The display list's own spelling, because the same three numbers say where a style sheet's
/// `--effect-field` lands and where the shader's declaration is compared against Rust's. One type
/// for both is what keeps those two answers from drifting.
pub type ParamsField = zgui_scene::ShaderField;

impl ParamsLayout {
    /// The layout of an effect that declares no parameters.
    pub const EMPTY: Self = Self {
        size: 0,
        fields: &[],
    };
}

impl EffectProgram {
    /// An effect that draws nothing, for a test that needs a handle rather than a picture.
    pub const EMPTY: Self = Self {
        mode: ShaderMode::Paint,
        label: "zgui.effect.empty",
        representation: &[],
        source: "",
        params: ParamsLayout::EMPTY,
    };
}
