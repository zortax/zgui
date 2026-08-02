//! `font-variant-position`.

/// Whether the face's own raised or lowered forms are used.
///
/// Distinct from a synthesised superscript, which is the same glyph drawn smaller and moved: this
/// selects a *different glyph* that the face drew for the purpose, so it changes the shaped result
/// rather than the transform applied to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontVariantPosition {
    /// `normal` — the ordinary forms.
    #[default]
    Normal,
    /// `sub` — the face's subscript forms.
    Sub,
    /// `super` — the face's superscript forms.
    Super,
}
