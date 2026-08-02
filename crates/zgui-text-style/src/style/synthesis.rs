//! `font-synthesis-weight`.

/// Whether a bolder face may be faked when the family has none.
///
/// A shaper asked for a weight the family does not cover has two answers: draw the weight it has,
/// or thicken that weight's outlines until they look like the one asked for. Faking it widens every
/// glyph, so the choice changes advances and belongs to the shaping half of a style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SynthesisWeight {
    /// `auto` — a bolder weight may be faked.
    #[default]
    Auto,
    /// `none` — it may not; the nearest real weight is drawn instead.
    None,
}
