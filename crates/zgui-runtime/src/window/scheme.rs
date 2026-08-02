//! Which colour scheme the window is presented in, and how a change of it reaches the pixels.
//!
//! The desktop's light-or-dark preference is a *level*, exactly like the surface's extent: the
//! platform reports what it is now, and the window carries it until it is reported to be something
//! else. It is held here rather than derived from the event that announced it because the event is
//! a notification and the frame that answers it may run much later, after several more have
//! arrived.
//!
//! # Why this is not merely a field
//!
//! `prefers-color-scheme` is the input to a media query, and a media query decides *which rules
//! match*. So the preference has to reach the cascade's device before the frame that follows it is
//! styled, and it reaches the device only by way of the viewport the device is built from. A
//! preference stored anywhere else is a preference no rule can read: the whole application stays in
//! the scheme it launched in, through every repaint, for as long as it runs.
//!
//! The scheme is kept *beside* the viewport rather than only inside it because the viewport is
//! rebuilt from the surface's extent whenever the surface moves, and a value that only lives in the
//! thing being rebuilt is a value the next resize silently discards.

use crate::window::Window;

impl Window {
    /// The colour scheme this window's document is being styled against.
    pub fn color_scheme(&self) -> zgui_style::ColorScheme {
        self.scheme
    }

    /// Presents this window in `scheme`, and reports whether that moved.
    ///
    /// Reporting the move is what keeps a desktop that re-states the preference it already had from
    /// costing a frame — and, more importantly, what stops it from costing a *device rebuild*,
    /// which dirties every origin whose media queries mention the scheme and restyles the document
    /// underneath them.
    pub fn set_color_scheme(&mut self, scheme: zgui_style::ColorScheme) -> bool {
        if self.scheme == scheme {
            return false;
        }
        self.scheme = scheme;
        self.viewport = self.viewport.in_scheme(scheme);
        true
    }

    /// The same, taking the platform's spelling of the preference.
    pub(crate) fn set_platform_color_scheme(&mut self, scheme: zgui_platform::ColorScheme) -> bool {
        self.set_color_scheme(from_platform(scheme))
    }
}

/// The style engine's spelling of the platform's preference.
pub(crate) fn from_platform(scheme: zgui_platform::ColorScheme) -> zgui_style::ColorScheme {
    match scheme {
        zgui_platform::ColorScheme::Dark => zgui_style::ColorScheme::Dark,
        _ => zgui_style::ColorScheme::Light,
    }
}

#[cfg(test)]
mod tests {
    use super::from_platform;

    #[test]
    fn both_preferences_survive_the_crossing() {
        assert_eq!(
            from_platform(zgui_platform::ColorScheme::Dark),
            zgui_style::ColorScheme::Dark
        );
        assert_eq!(
            from_platform(zgui_platform::ColorScheme::Light),
            zgui_style::ColorScheme::Light
        );
    }
}
