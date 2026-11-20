//! Which coverage a window draws its text with.
//!
//! Text with one coverage for each colour channel is sharper on a display with three horizontal
//! stripes for each pixel, and shows coloured edges on every other display. The platform states
//! the layout where it knows it, the application or the user can state it, and one function
//! decides from both.

use zgui_platform::SubpixelLayout;
use zgui_render::SubpixelOrder;

/// The environment variable that states [`TextAntialiasing`] for every window of a process.
const VARIABLE: &str = "ZGUI_TEXT_AA";

/// The scale from which a display of unknown layout gets one coverage for each pixel.
///
/// At this density the coloured edges add no visible detail, and a display this dense is often
/// one with no stripes at all.
const DENSE_SCALE: f32 = 2.0;

/// How a window antialiases its text.
///
/// ```
/// use zgui_runtime::TextAntialiasing;
///
/// assert_eq!(TextAntialiasing::default(), TextAntialiasing::Auto);
/// assert_eq!(TextAntialiasing::parse("gray"), Some(TextAntialiasing::Grayscale));
/// assert_eq!(TextAntialiasing::parse("BGR"), Some(TextAntialiasing::Bgr));
/// assert_eq!(TextAntialiasing::parse("lcd"), None);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAntialiasing {
    /// Follows the layout the platform reports for the display.
    ///
    /// A display of unknown layout gets per-channel coverage in red, green, blue order below a
    /// scale of two, and one coverage for each pixel from a scale of two.
    #[default]
    Auto,
    /// One coverage for each pixel. Correct on every display.
    Grayscale,
    /// One coverage for each colour channel, for stripes that run red, green, blue.
    Rgb,
    /// One coverage for each colour channel, for stripes that run blue, green, red.
    Bgr,
}

impl TextAntialiasing {
    /// Reads a setting from its name: `auto`, `gray`, `grayscale`, `rgb` or `bgr`, in any case.
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "gray" | "grey" | "grayscale" | "greyscale" => Some(Self::Grayscale),
            "rgb" => Some(Self::Rgb),
            "bgr" => Some(Self::Bgr),
            _ => None,
        }
    }

    /// The setting the `ZGUI_TEXT_AA` environment variable states, when it states one.
    ///
    /// A value that is no setting is reported once and ignored.
    pub fn from_env() -> Option<Self> {
        static READ: std::sync::OnceLock<Option<TextAntialiasing>> = std::sync::OnceLock::new();
        *READ.get_or_init(|| {
            let value = std::env::var(VARIABLE).ok()?;
            let parsed = Self::parse(&value);
            if parsed.is_none() {
                tracing::warn!(
                    target: "zgui::runtime",
                    "{VARIABLE}={value:?} is ignored: the values are auto, gray, rgb and bgr"
                );
            }
            parsed
        })
    }

    /// The stripe order a window draws per-channel coverage for, or `None` for one coverage for
    /// each pixel.
    ///
    /// `layout` is what the platform reports for the display, and `scale` is how many device
    /// pixels one CSS pixel is.
    ///
    /// ```
    /// use zgui_platform::SubpixelLayout;
    /// use zgui_render::SubpixelOrder;
    /// use zgui_runtime::TextAntialiasing;
    ///
    /// let auto = TextAntialiasing::Auto;
    /// assert_eq!(auto.order(SubpixelLayout::Unknown, 1.0), Some(SubpixelOrder::RedToBlue));
    /// assert_eq!(auto.order(SubpixelLayout::Unknown, 2.0), None);
    /// assert_eq!(auto.order(SubpixelLayout::HorizontalBgr, 2.0), Some(SubpixelOrder::BlueToRed));
    /// assert_eq!(auto.order(SubpixelLayout::None, 1.0), None);
    /// assert_eq!(TextAntialiasing::Grayscale.order(SubpixelLayout::HorizontalRgb, 1.0), None);
    /// ```
    pub fn order(self, layout: SubpixelLayout, scale: f32) -> Option<SubpixelOrder> {
        match self {
            Self::Grayscale => None,
            Self::Rgb => Some(SubpixelOrder::RedToBlue),
            Self::Bgr => Some(SubpixelOrder::BlueToRed),
            Self::Auto => match layout {
                SubpixelLayout::HorizontalRgb => Some(SubpixelOrder::RedToBlue),
                SubpixelLayout::HorizontalBgr => Some(SubpixelOrder::BlueToRed),
                SubpixelLayout::Unknown if scale < DENSE_SCALE => Some(SubpixelOrder::RedToBlue),
                _ => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_platform::SubpixelLayout;
    use zgui_render::SubpixelOrder;

    use super::TextAntialiasing;

    #[test]
    fn a_stated_setting_ignores_the_display() {
        for layout in [
            SubpixelLayout::Unknown,
            SubpixelLayout::None,
            SubpixelLayout::HorizontalRgb,
            SubpixelLayout::HorizontalBgr,
        ] {
            for scale in [1.0, 1.5, 2.0, 3.0] {
                assert_eq!(TextAntialiasing::Grayscale.order(layout, scale), None);
                assert_eq!(
                    TextAntialiasing::Rgb.order(layout, scale),
                    Some(SubpixelOrder::RedToBlue)
                );
                assert_eq!(
                    TextAntialiasing::Bgr.order(layout, scale),
                    Some(SubpixelOrder::BlueToRed)
                );
            }
        }
    }

    #[test]
    fn an_unknown_display_gets_stripes_only_below_a_scale_of_two() {
        let auto = TextAntialiasing::Auto;
        for scale in [1.0, 1.25, 1.5, 1.75] {
            assert_eq!(
                auto.order(SubpixelLayout::Unknown, scale),
                Some(SubpixelOrder::RedToBlue),
                "{scale}"
            );
        }
        for scale in [2.0, 2.5, 3.0] {
            assert_eq!(auto.order(SubpixelLayout::Unknown, scale), None, "{scale}");
        }
    }

    #[test]
    fn a_reported_layout_decides_at_every_scale() {
        let auto = TextAntialiasing::Auto;
        for scale in [1.0, 2.0, 3.0] {
            assert_eq!(
                auto.order(SubpixelLayout::HorizontalRgb, scale),
                Some(SubpixelOrder::RedToBlue)
            );
            assert_eq!(
                auto.order(SubpixelLayout::HorizontalBgr, scale),
                Some(SubpixelOrder::BlueToRed)
            );
            assert_eq!(auto.order(SubpixelLayout::None, scale), None);
        }
    }
}
