//! Naming an application's shader, rather than saying what it is.
//!
//! # Why a name and not a program
//!
//! A shader is WGSL, and WGSL is one graphics API's language. The display list names no graphics
//! API — a second renderer over a different one is an ordinary implementation rather than a fork —
//! so what a primitive carries here is a [`ShaderId`] and nothing else, exactly as
//! [`ExternalTextureId`](crate::ExternalTextureId) names a texture the renderer did not draw. A
//! renderer keeps its own registry keyed by the handle and resolves it when the frame is drawn,
//! and a renderer that has never heard of the handle draws nothing rather than drawing wrongly.
//!
//! # Why parameters are interned
//!
//! Parameters are bound beside the draw rather than read per instance, so two rectangles with
//! different parameters cannot be one draw call. Interning is what stops that mattering: every
//! element of a document that resolved to the same parameters gets the same slot and therefore one
//! draw between them, which is the difference between a smoothed corner costing nothing and
//! costing a draw call per card.

mod declared;
mod mode;
mod params;

pub use crate::shader::declared::{
    ShaderDeclaration, ShaderField, by_id as shader_declared_by_id, count as declared_count,
    declare as declare_shader, named as shader_named, property,
};
pub use crate::shader::mode::{ShaderMode, ShaderReads};
pub use crate::shader::params::{
    MAX_PARAMS_BYTES, ShaderParams, ShaderParamsSlot, ShaderParamsTable,
};

/// What every effect in a frame is told about the frame itself.
///
/// It is the one thing an effect reads that the document does not hold. Nothing the framework
/// draws reads it, which is why it travels on the scene rather than through a stage of its own.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameClock {
    /// Seconds since the document started.
    pub seconds: f32,
    /// Seconds the previous frame took.
    pub delta: f32,
    /// Device pixels per CSS pixel.
    pub scale: f32,
}

impl FrameClock {
    /// The clock as the four floats the shading block holds.
    pub fn to_lane(self) -> [f32; 4] {
        [self.seconds, self.delta, self.scale, 0.0]
    }
}

/// An application shader the renderer was told about.
///
/// Opaque here on purpose. What a shader *is* — which language, which device, which pipeline — is
/// the renderer's knowledge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShaderId(pub u32);

impl ShaderId {
    /// The handle no effect is registered under, which draws nothing.
    pub const NONE: Self = Self(0);

    /// The handle's numeric value, for indexing and for transcripts.
    pub const fn index(self) -> u32 {
        self.0
    }

    /// Whether this names an effect at all.
    pub const fn is_some(self) -> bool {
        self.0 != Self::NONE.0
    }
}

#[cfg(test)]
mod tests {
    use super::ShaderId;

    #[test]
    fn the_absent_handle_names_nothing() {
        assert!(!ShaderId::NONE.is_some());
        assert!(ShaderId(1).is_some());
    }
}
