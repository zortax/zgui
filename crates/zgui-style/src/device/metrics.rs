//! The adapter that lets the cascade ask a font system how tall an `ex` is.
//!
//! The style engine wants a boxed provider **by value** inside every device, and a device is
//! rebuilt whenever the surface changes: a resize, a scale change or a colour-scheme flip all
//! construct a new one. A provider that owned its memo would therefore throw the memo away exactly
//! when the whole document is about to restyle against it.
//!
//! So the provider stored in the device is a thin adapter holding a shared handle to the real
//! source. The handle is created once, when the document is, and cloned into every device built
//! afterwards — one lock and one memo for the life of the document, however many devices come and
//! go.

use std::sync::Arc;

use style::device::servo::FontMetricsProvider;
use style::font_metrics::FontMetrics;
use style::properties::style_structs::Font;
use style::values::computed::font::GenericFontFamily;
use style::values::computed::{CSSPixelLength, Length};
use style::values::specified::font::QueryFontMetricsFlags;
use zgui_geom::CssPx;
use zgui_text::{FaceMetrics, FaceQuery, FontMetricsSource};
use zgui_text_style::lower::font;
use zgui_text_style::{FontFamilyList, GenericFamily};

/// Answers the cascade's font-metric questions out of a shared source.
///
/// One of these is boxed into every device, and every one of them answers from the same source, so
/// the source's memo survives each device the surface's lifetime produces.
#[derive(Clone)]
pub struct MetricsAdapter {
    /// Where the answers come from.
    source: Arc<dyn FontMetricsSource>,
}

impl MetricsAdapter {
    /// An adapter answering out of `source`.
    pub fn new(source: Arc<dyn FontMetricsSource>) -> Self {
        Self { source }
    }

    /// The source the answers come from.
    pub fn source(&self) -> &Arc<dyn FontMetricsSource> {
        &self.source
    }
}

impl core::fmt::Debug for MetricsAdapter {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("MetricsAdapter")
    }
}

impl FontMetricsProvider for MetricsAdapter {
    /// Metrics for the face `font` selects, at `base_size`.
    ///
    /// `flags` is not consulted. It names the metrics the caller is about to read, which is an
    /// invitation to compute less; the source answers all seven from one face lookup, so honouring
    /// it would save nothing and would make two callers asking for different subsets of the same
    /// face disagree about what was cached.
    fn query_font_metrics(
        &self,
        vertical: bool,
        font: &Font,
        base_size: CSSPixelLength,
        _flags: QueryFontMetricsFlags,
    ) -> FontMetrics {
        // Owned, because the query borrows them and both are built out of the group rather than
        // stored in it.
        let family: FontFamilyList = font::family(font);
        let variations = font::variations(font);
        let query = FaceQuery {
            family: &family,
            weight: font::weight(font),
            slant: font::slant(font),
            width: font::width(font),
            variations: &variations,
            language: font::language(font),
        };
        to_engine(
            self.source
                .face_metrics(&query, CssPx(base_size.px()), vertical),
        )
    }

    /// The size an unstyled document starts from for one generic family.
    ///
    /// The cascade's internal "no generic was named" placeholder is answered as the proportional
    /// default, which is the family an unstyled document resolves to.
    fn base_size_for_generic(&self, generic: GenericFontFamily) -> Length {
        let generic = font::generic_family(generic).unwrap_or(GenericFamily::SansSerif);
        Length::new(self.source.base_size(generic).0)
    }
}

/// This framework's face metrics in the engine's own shape, field for field.
fn to_engine(metrics: FaceMetrics) -> FontMetrics {
    FontMetrics {
        x_height: metrics.x_height.map(length),
        zero_advance_measure: metrics.zero_advance.map(length),
        cap_height: metrics.cap_height.map(length),
        ic_width: metrics.ic_width.map(length),
        ascent: length(metrics.ascent),
        script_percent_scale_down: metrics.script_percent,
        script_script_percent_scale_down: metrics.script_script_percent,
    }
}

/// One CSS-pixel length in the engine's spelling of the same thing.
fn length(value: CssPx) -> Length {
    Length::new(value.0)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use style::device::servo::FontMetricsProvider;
    use style::properties::style_structs::Font;
    use style::values::computed::CSSPixelLength;
    use style::values::computed::font::GenericFontFamily;
    use style::values::specified::font::QueryFontMetricsFlags;
    use zgui_text::{FixedMetrics, FontMetricsSource};

    use super::MetricsAdapter;

    #[test]
    fn every_one_of_the_seven_fields_reaches_the_engines_shape() {
        let adapter = MetricsAdapter::new(Arc::new(FixedMetrics::new()));
        let metrics = adapter.query_font_metrics(
            false,
            &Font::initial_values(),
            CSSPixelLength::new(20.0),
            QueryFontMetricsFlags::empty(),
        );

        let ours = FixedMetrics::at(zgui_geom::CssPx(20.0));
        assert_eq!(metrics.x_height.map(|l| l.px()), ours.x_height.map(|p| p.0));
        assert_eq!(
            metrics.zero_advance_measure.map(|l| l.px()),
            ours.zero_advance.map(|p| p.0)
        );
        assert_eq!(
            metrics.cap_height.map(|l| l.px()),
            ours.cap_height.map(|p| p.0)
        );
        assert_eq!(metrics.ic_width.map(|l| l.px()), ours.ic_width.map(|p| p.0));
        assert_eq!(metrics.ascent.px(), ours.ascent.0);
        assert_eq!(metrics.script_percent_scale_down, ours.script_percent);
        assert_eq!(
            metrics.script_script_percent_scale_down,
            ours.script_script_percent
        );
    }

    #[test]
    fn the_base_size_of_a_monospace_family_is_not_the_proportional_one() {
        let source = FixedMetrics::new();
        let adapter = MetricsAdapter::new(Arc::new(source));
        let monospace = adapter.base_size_for_generic(GenericFontFamily::Monospace);
        let proportional = adapter.base_size_for_generic(GenericFontFamily::SansSerif);
        assert_ne!(monospace.px(), proportional.px());
        assert_eq!(
            monospace.px(),
            source
                .base_size(zgui_text_style::GenericFamily::Monospace)
                .0
        );
        // The placeholder the cascade uses for "no generic named" answers as the proportional
        // default rather than panicking or reporting zero.
        assert_eq!(
            adapter.base_size_for_generic(GenericFontFamily::None).px(),
            proportional.px()
        );
    }

    #[test]
    fn the_memo_behind_the_adapter_is_shared_rather_than_rebuilt_per_device() {
        let source: Arc<dyn FontMetricsSource> = Arc::new(FixedMetrics::new());
        let first = MetricsAdapter::new(Arc::clone(&source));
        let second = MetricsAdapter::new(Arc::clone(&source));
        assert!(Arc::ptr_eq(first.source(), second.source()));
    }
}
