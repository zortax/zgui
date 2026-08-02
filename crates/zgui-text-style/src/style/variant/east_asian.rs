//! `font-variant-east-asian`.

/// Which national or legacy form of a CJK character a face is asked for.
///
/// The choices are mutually exclusive in the grammar, which is why this is an enumeration rather
/// than a set of flags: a face cannot draw a character as both a 1978 and a 2004 form at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EastAsianForms {
    /// Not asked for: the face's default forms.
    #[default]
    Auto,
    /// `jis78`.
    Jis78,
    /// `jis83`.
    Jis83,
    /// `jis90`.
    Jis90,
    /// `jis04`.
    Jis04,
    /// `simplified`.
    Simplified,
    /// `traditional`.
    Traditional,
}

/// How much room each CJK glyph takes.
///
/// A shaping-side choice with visible consequences for width: full-width forms occupy one em each
/// whatever they draw, so the same run of Latin characters inside CJK text measures differently
/// under the two settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EastAsianWidth {
    /// Not asked for.
    #[default]
    Auto,
    /// `full-width` — one em per glyph.
    FullWidth,
    /// `proportional-width` — each glyph as wide as it needs to be.
    ProportionalWidth,
}

/// `font-variant-east-asian`, split into the three independent choices the grammar allows at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontVariantEastAsian {
    /// The national or legacy form.
    pub forms: EastAsianForms,
    /// The glyph width.
    pub width: EastAsianWidth,
    /// `ruby` — the smaller forms drawn as an annotation beside a base character.
    pub ruby: bool,
}

impl FontVariantEastAsian {
    /// `normal`: nothing asked for.
    pub const NORMAL: Self = Self {
        forms: EastAsianForms::Auto,
        width: EastAsianWidth::Auto,
        ruby: false,
    };
}
