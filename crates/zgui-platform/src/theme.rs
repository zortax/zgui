//! The light or dark preference the desktop has expressed.

/// Whether the user has asked for light or dark surfaces.
///
/// This is a *preference*, not a palette. It is the input to a style sheet's light-and-dark query,
/// and nothing here decides what any colour is.
///
/// Not every platform has an answer. Where the preference cannot be discovered at all the answer
/// is absent rather than guessed, because guessing "light" and being wrong produces a white flash
/// on every launch for every user who chose dark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ColorScheme {
    /// The user prefers light surfaces with dark text.
    #[default]
    Light,
    /// The user prefers dark surfaces with light text.
    Dark,
}

impl ColorScheme {
    /// Whether this is the dark preference.
    pub const fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }

    /// The other preference.
    pub const fn inverted(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ColorScheme;

    #[test]
    fn inverting_twice_returns_the_original() {
        for scheme in [ColorScheme::Light, ColorScheme::Dark] {
            assert_eq!(scheme.inverted().inverted(), scheme);
        }
        assert!(ColorScheme::Dark.is_dark());
        assert!(!ColorScheme::Light.is_dark());
    }
}
