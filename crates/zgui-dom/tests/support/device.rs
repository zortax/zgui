//! The viewport, the pixel ratio and the font metrics length units resolve against.

use euclid::Scale;
use selectors::matching::QuirksMode;
use style::device::Device;
use style::device::servo::FontMetricsProvider;
use style::font_metrics::FontMetrics;
use style::media_queries::MediaType;
use style::properties::ComputedValues;
use style::properties::style_structs::Font;
use style::queries::values::PrefersColorScheme;
use style::servo::media_features::PointerCapabilities;
use style::values::computed::font::GenericFontFamily;
use style::values::computed::{CSSPixelLength, Length};
use style::values::specified::font::QueryFontMetricsFlags;

/// A font metrics source with no fonts behind it.
///
/// The real one queries the font stack. This returns the initial metrics, which is enough for
/// everything except the units measured against a font's own glyphs, and no rule in these cases uses
/// one.
#[derive(Debug)]
pub(crate) struct StubMetrics;

impl FontMetricsProvider for StubMetrics {
    fn query_font_metrics(
        &self,
        _vertical: bool,
        _font: &Font,
        _base_size: CSSPixelLength,
        _flags: QueryFontMetricsFlags,
    ) -> FontMetrics {
        FontMetrics::default()
    }

    fn base_size_for_generic(&self, _generic: GenericFontFamily) -> Length {
        Length::new(16.0)
    }
}

/// A size in whichever unit the caller's binding asks for.
///
/// The device constructor takes two differently united sizes and a scale; two of the three are
/// reachable by inference and the scale is not, which is the whole reason this crate names the
/// geometry library the engine uses.
fn sized<S: From<(f32, f32)>>(width: f32, height: f32) -> S {
    S::from((width, height))
}

/// A device for a `width` by `height` CSS-pixel viewport at `dppx` device pixels per CSS pixel.
pub(crate) fn device(width: f32, height: f32, dppx: f32) -> Device {
    Device::new(
        MediaType::screen(),
        QuirksMode::NoQuirks,
        sized(width, height),
        sized(width * dppx, height * dppx),
        Scale::new(dppx),
        Box::new(StubMetrics),
        ComputedValues::initial_values_with_font_override(Font::initial_values()),
        PrefersColorScheme::Light,
        PointerCapabilities::FINE | PointerCapabilities::HOVER,
        PointerCapabilities::FINE | PointerCapabilities::HOVER,
    )
}
