//! The application effects a style sheet names, and what each one declares.
//!
//! An effect is declared once while the application starts, under a name a style sheet writes in
//! one of the [`property`] custom properties. The declaration says what the effect does, what it
//! reads and where its parameters are. Layout reads it for how far a filter reaches, and painting
//! reads it to decide what the effect's box draws.

mod declared;
mod mode;

pub use crate::shader::declared::{ShaderDeclaration, ShaderField, declare, named, property};
pub use crate::shader::mode::{ShaderMode, ShaderReads};

/// A declared application effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShaderId(pub u32);

impl ShaderId {
    /// The handle no effect is declared under.
    pub const NONE: Self = Self(0);

    /// Returns the handle's numeric value.
    pub const fn index(self) -> u32 {
        self.0
    }

    /// Reports whether this handle names an effect.
    pub const fn is_some(self) -> bool {
        self.0 != Self::NONE.0
    }
}
