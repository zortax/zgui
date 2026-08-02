//! Reading the seven cascade metrics out of one face.

use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};
use zgui_geom::CssPx;
use zgui_text::FaceMetrics;
use zgui_text_style::FontVariation;

/// The character `ch` measures against.
const ZERO: char = '0';

/// The character `ic` measures against — the CJK water ideograph, which is square by definition.
const IDEOGRAPH: char = '\u{6c34}';

/// Reads the metrics one face reports at one size.
///
/// Every optional field is `None` when the face genuinely does not carry the metric, never zero.
/// The distinction is the whole point of the type: a unit whose metric is absent resolves against
/// a documented fraction of the font size, while a zero would collapse it.
///
/// # What `vertical` does and does not change
///
/// The four unit metrics — x-height, zero advance, cap height and ideograph width — are the same
/// measurements in either writing mode, and the fifth, the ascent, is read from the horizontal
/// metrics in both. A face's vertical typographic ascent is a separate measurement and is not read
/// here; reporting the horizontal one is the closer answer, and reporting zero would collapse
/// every line box in a vertical document.
pub(crate) fn face_metrics(
    data: &[u8],
    index: u32,
    size: CssPx,
    variations: &[FontVariation],
    _vertical: bool,
) -> Option<FaceMetrics> {
    let font = FontRef::from_index(data, index).ok()?;
    let coords = normalised(&font, variations);
    let location = LocationRef::new(&coords);
    let scale = Size::new(size.0);
    let metrics = font.metrics(scale, location);
    let charmap = font.charmap();
    let glyphs = font.glyph_metrics(scale, location);
    let advance = |character: char| {
        charmap
            .map(character)
            .and_then(|glyph| glyphs.advance_width(glyph))
            .map(CssPx)
    };
    Some(FaceMetrics {
        x_height: metrics.x_height.map(CssPx),
        zero_advance: advance(ZERO),
        cap_height: metrics.cap_height.map(CssPx),
        ic_width: advance(IDEOGRAPH),
        ascent: CssPx(metrics.ascent),
        // The two script scale-downs live in the `MATH` table, which is read for mathematical
        // typesetting and is absent from every text face. Reporting them as absent is the honest
        // answer and resolves each dependent unit against its documented fallback.
        script_percent: None,
        script_script_percent: None,
    })
}

/// The normalised axis coordinates a variation list instances the face at.
fn normalised(
    font: &FontRef<'_>,
    variations: &[FontVariation],
) -> Vec<skrifa::instance::NormalizedCoord> {
    if variations.is_empty() {
        return Vec::new();
    }
    let settings = variations.iter().map(|variation| {
        (
            skrifa::Tag::from_be_bytes(variation.tag.to_be_bytes()),
            variation.value,
        )
    });
    font.axes().location(settings).coords().to_vec()
}
