//! What an effect does, and what it reads.

/// What an effect does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShaderMode {
    /// The effect shades the box's colour, inside the box and its clip.
    Paint,
    /// The effect computes the box's coverage, and the box's own paints fill what it covers.
    Coverage,
    /// The effect is a step of the box's filter chain, and reads the content drawn before it.
    Filter,
}

/// The inputs an effect reads that change while the document stays the same.
///
/// A declared read makes the effect's box repaint when that input changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShaderReads {
    /// Whether the effect reads the clock.
    pub time: bool,
    /// Whether the effect reads the pointer.
    pub pointer: bool,
}

impl ShaderReads {
    /// An effect that reads neither the clock nor the pointer.
    pub const NOTHING: Self = Self {
        time: false,
        pointer: false,
    };
}
