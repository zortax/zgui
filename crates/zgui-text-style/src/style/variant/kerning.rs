//! `font-kerning`.

/// Whether the face's own kerning is applied.
///
/// Kerning moves one glyph relative to the one beside it, so this changes advances and belongs to
/// the shaping half of a style rather than the breaking half.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontKerning {
    /// `auto` — whatever the face and the script call for, which is the shaper's own judgement.
    #[default]
    Auto,
    /// `normal` — apply kerning.
    Normal,
    /// `none` — do not.
    None,
}
