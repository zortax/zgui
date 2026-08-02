//! `font-variant-ligatures`.

/// One ligature group's setting.
///
/// Three states rather than two, and the third is not redundant: leaving a group alone is different
/// from turning it on. Common ligatures are on by default in most faces and contextual alternates
/// almost always are, so `auto` means "whatever the face and the shaper would do", which for those
/// two is *on* — an author who writes nothing gets them, and an author who writes
/// `no-common-ligatures` does not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LigatureSetting {
    /// Not mentioned: the face's and the shaper's own default for the group.
    #[default]
    Auto,
    /// Explicitly on.
    On,
    /// Explicitly off.
    Off,
}

/// `font-variant-ligatures`, one setting per group.
///
/// `none` is not a variant of its own: it is the value in which all four groups are
/// [`LigatureSetting::Off`], which is what [`FontVariantLigatures::none`] builds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontVariantLigatures {
    /// `common-ligatures` / `no-common-ligatures` — `fi`, `fl` and the rest of the everyday set.
    pub common: LigatureSetting,
    /// `discretionary-ligatures` / `no-discretionary-ligatures` — the decorative ones a face offers
    /// but does not apply unasked.
    pub discretionary: LigatureSetting,
    /// `historical-ligatures` / `no-historical-ligatures` — forms that were once standard.
    pub historical: LigatureSetting,
    /// `contextual` / `no-contextual` — substitutions that depend on the neighbouring glyphs.
    pub contextual: LigatureSetting,
}

impl FontVariantLigatures {
    /// `normal`: every group left to the face.
    pub const NORMAL: Self = Self {
        common: LigatureSetting::Auto,
        discretionary: LigatureSetting::Auto,
        historical: LigatureSetting::Auto,
        contextual: LigatureSetting::Auto,
    };

    /// `none`: every group turned off.
    pub const fn none() -> Self {
        Self {
            common: LigatureSetting::Off,
            discretionary: LigatureSetting::Off,
            historical: LigatureSetting::Off,
            contextual: LigatureSetting::Off,
        }
    }
}
