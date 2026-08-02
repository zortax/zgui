//! The desktop's light or dark preference, where it can be discovered at all.

use winit::window::Theme;
use zgui_platform::ColorScheme;

/// The preference a windowing theme stands for.
pub(crate) const fn scheme(theme: Theme) -> ColorScheme {
    match theme {
        Theme::Light => ColorScheme::Light,
        Theme::Dark => ColorScheme::Dark,
    }
}

/// The theme a preference stands for, for the window attribute that takes one.
pub(crate) const fn theme(scheme: ColorScheme) -> Theme {
    match scheme {
        ColorScheme::Dark => Theme::Dark,
        _ => Theme::Light,
    }
}

#[cfg(test)]
mod tests {
    use super::{scheme, theme};
    use winit::window::Theme;
    use zgui_platform::ColorScheme;

    #[test]
    fn a_preference_survives_the_round_trip_in_both_directions() {
        for preference in [ColorScheme::Light, ColorScheme::Dark] {
            assert_eq!(scheme(theme(preference)), preference);
        }
        for platform in [Theme::Light, Theme::Dark] {
            assert_eq!(theme(scheme(platform)), platform);
        }
    }

    #[test]
    fn dark_is_never_read_as_light() {
        assert!(scheme(Theme::Dark).is_dark());
        assert!(!scheme(Theme::Light).is_dark());
    }
}
