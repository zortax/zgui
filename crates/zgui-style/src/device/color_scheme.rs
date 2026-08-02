//! Which colour scheme the surface is being presented in.

use style::queries::values::PrefersColorScheme;

/// The colour scheme a document is styled for.
///
/// This is what `prefers-color-scheme` answers, and it is an input rather than a preference: the
/// platform reports what the user asked the system for, and an application may override it per
/// window. Both arrive here the same way.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum ColorScheme {
    /// Light surfaces, dark text.
    #[default]
    Light,
    /// Dark surfaces, light text.
    Dark,
}

impl ColorScheme {
    /// The engine's spelling of this scheme.
    pub(crate) fn to_engine(self) -> PrefersColorScheme {
        match self {
            Self::Light => PrefersColorScheme::Light,
            Self::Dark => PrefersColorScheme::Dark,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ColorScheme;
    use style::queries::values::PrefersColorScheme;

    #[test]
    fn both_schemes_map_to_distinct_engine_values() {
        assert_eq!(ColorScheme::Light.to_engine(), PrefersColorScheme::Light);
        assert_eq!(ColorScheme::Dark.to_engine(), PrefersColorScheme::Dark);
        assert_eq!(ColorScheme::default(), ColorScheme::Light);
    }
}
