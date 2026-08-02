//! Light, dark, and letting the desktop decide.

/// Which colour scheme an interface is presented in.
///
/// ```
/// use zgui_ui_tokens::ColorScheme;
///
/// assert_eq!(ColorScheme::default(), ColorScheme::System);
/// assert_eq!(ColorScheme::Dark.name(), "dark");
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum ColorScheme {
    /// Light surfaces, dark text.
    Light,
    /// Dark surfaces, light text.
    Dark,
    /// Whichever the desktop asked for.
    ///
    /// This resolves in the style sheet rather than in Rust: the theme is written out with the
    /// light tokens and a `prefers-color-scheme: dark` block over them, so the answer comes from
    /// the same media query every other rule in the document is matched against and changes when
    /// the desktop's setting does — with no signal, no portal call and no theme rebuild.
    #[default]
    System,
}

impl ColorScheme {
    /// Every scheme, in the order they are written.
    pub const ALL: &'static [Self] = &[Self::Light, Self::Dark, Self::System];

    /// How this scheme is written as an attribute value.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    /// Whether this scheme needs the dark tokens written out at all.
    pub const fn wants_dark(self) -> bool {
        matches!(self, Self::Dark | Self::System)
    }

    /// Whether this scheme needs the light tokens written out at all.
    pub const fn wants_light(self) -> bool {
        matches!(self, Self::Light | Self::System)
    }
}

#[cfg(test)]
mod tests {
    use super::ColorScheme;

    #[test]
    fn every_scheme_writes_at_least_one_set_of_tokens() {
        for scheme in ColorScheme::ALL {
            assert!(
                scheme.wants_light() || scheme.wants_dark(),
                "{scheme:?} writes nothing at all"
            );
        }
    }

    #[test]
    fn only_the_deferred_scheme_writes_both() {
        assert!(ColorScheme::System.wants_light() && ColorScheme::System.wants_dark());
        assert!(!ColorScheme::Light.wants_dark());
        assert!(!ColorScheme::Dark.wants_light());
    }

    #[test]
    fn every_scheme_has_a_distinct_name() {
        let mut names: Vec<&str> = ColorScheme::ALL
            .iter()
            .map(|scheme| scheme.name())
            .collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ColorScheme::ALL.len());
    }
}
