//! Constructing the device one frame's surface is styled against.

use std::sync::Arc;

use euclid::Scale;
use selectors::matching::QuirksMode;
use style::device::Device;
use style::media_queries::MediaType;
use style::properties::ComputedValues;
use style::properties::style_structs::Font;
use style::servo::media_features::PointerCapabilities;
use zgui_text::FontMetricsSource;

use crate::device::metrics::MetricsAdapter;
use crate::device::viewport::Viewport;

/// The pointer capabilities a desktop surface reports.
///
/// Stated once rather than at each of the two places the engine asks for them, because the primary
/// pointer and the union of all pointers are the same set on a surface with one mouse, and a
/// difference between them would be a decision rather than a repetition.
const POINTERS: PointerCapabilities = PointerCapabilities::FINE.union(PointerCapabilities::HOVER);

/// Builds the device for `viewport`, answering font-metric questions out of `metrics`.
///
/// The returned device carries a fresh set of "was this ever read" flags, which is why the flag a
/// caller cares about has to be read off the device being replaced rather than off this one.
pub fn build(viewport: Viewport, metrics: &Arc<dyn FontMetricsSource>) -> Device {
    Device::new(
        MediaType::screen(),
        QuirksMode::NoQuirks,
        sized(viewport.width.0, viewport.height.0),
        sized(
            viewport.width.0 * viewport.scale,
            viewport.height.0 * viewport.scale,
        ),
        Scale::new(viewport.scale),
        Box::new(MetricsAdapter::new(Arc::clone(metrics))),
        ComputedValues::initial_values_with_font_override(Font::initial_values()),
        viewport.scheme.to_engine(),
        POINTERS,
        POINTERS,
    )
}

/// A size in whichever of the engine's two tagged sizes the binding site asks for.
///
/// The device constructor takes one size in CSS pixels and one in device pixels, and the tag is
/// decided by the parameter rather than by the caller, so the conversion is written once.
fn sized<S: From<(f32, f32)>>(width: f32, height: f32) -> S {
    S::from((width, height))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zgui_geom::CssPx;
    use zgui_text::{FixedMetrics, FontMetricsSource};

    use super::build;
    use crate::device::color_scheme::ColorScheme;
    use crate::device::viewport::Viewport;

    #[test]
    fn the_device_carries_the_surface_it_was_built_for() {
        let metrics: Arc<dyn FontMetricsSource> = Arc::new(FixedMetrics::new());
        let viewport = Viewport::new(CssPx(1280.0), CssPx(800.0))
            .at_scale(2.0)
            .in_scheme(ColorScheme::Dark);
        let device = build(viewport, &metrics);

        assert_eq!(device.viewport_size().width, 1280.0);
        assert_eq!(device.device_size().width, 2560.0);
        assert_eq!(device.device_pixel_ratio().0, 2.0);
        assert_eq!(
            device.color_scheme(),
            style::queries::values::PrefersColorScheme::Dark
        );
    }

    #[test]
    fn a_fresh_device_has_never_been_asked_for_a_viewport_unit() {
        // The flag that says a document resolved `vw` or `vh` belongs to the device that answered
        // the question, and a replacement starts it clear. Reading it off the new device is
        // therefore always `false`, which is why the epoch reads it off the outgoing one.
        let metrics: Arc<dyn FontMetricsSource> = Arc::new(FixedMetrics::new());
        let device = build(Viewport::new(CssPx(800.0), CssPx(600.0)), &metrics);
        assert!(!device.used_viewport_size());
    }
}
