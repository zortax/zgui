//! The strut: measured once per distinct text style, then remembered.

use rustc_hash::FxHashMap;
use zgui_geom::CssPx;
use zgui_text::StrutMetrics;
use zgui_text_style::{ShapingKey, TextStyle};

use crate::shape::brush::SlotBrush;
use crate::shape::style::LoweredStyle;

/// The one character the strut is measured with.
///
/// A block establishes a strut whether or not it holds any text, so the measurement cannot come
/// from the paragraph's own content. One ordinary lowercase letter is enough: every field read is
/// a property of the face at the size, not of the character.
const PROBE: &str = "x";

/// Struts already measured, keyed by the shaping half of the style they came from.
///
/// The shaping key is exactly the right key: it covers the family, size, weight, slant, width and
/// line height and nothing else, so two styles that differ only in colour or in alignment share
/// one measurement, and two that differ in anything a face would notice do not.
#[derive(Debug, Default)]
pub(crate) struct StrutCache {
    /// The measurements.
    entries: FxHashMap<ShapingKey, StrutMetrics>,
}

impl StrutCache {
    /// The strut for one style, measuring it if it has not been measured before.
    pub(crate) fn get_or_measure(
        &mut self,
        style: &TextStyle,
        fonts: &mut parley::FontContext,
        scratch: &mut parley::LayoutContext<SlotBrush>,
    ) -> StrutMetrics {
        let key = ShapingKey::of(style);
        if let Some(found) = self.entries.get(&key) {
            return *found;
        }
        let metrics = measure(style, fonts, scratch);
        self.entries.insert(key, metrics);
        metrics
    }

    /// Drops every measurement, which a newly registered face makes necessary.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Lays one character out and reads the face's contribution off the line it produced.
fn measure(
    style: &TextStyle,
    fonts: &mut parley::FontContext,
    scratch: &mut parley::LayoutContext<SlotBrush>,
) -> StrutMetrics {
    let lowered = LoweredStyle::of(style, zgui_scene::PaintSlot(0), CssPx::ZERO);
    let mut builder = scratch.ranged_builder(fonts, PROBE, 1.0, true);
    lowered.push_default(&mut builder);
    let mut layout: parley::Layout<SlotBrush> = builder.build(PROBE);
    layout.break_all_lines(None);
    let Some(line) = layout.get(0) else {
        return StrutMetrics {
            font_size: style.size,
            ..StrutMetrics::default()
        };
    };
    let metrics = line.metrics();
    let x_height = line
        .runs()
        .next()
        .and_then(|run| run.metrics().x_height)
        .unwrap_or(0.0);
    StrutMetrics {
        font_ascent: CssPx(metrics.ascent),
        font_descent: CssPx(metrics.descent),
        line_height: CssPx(metrics.line_height),
        x_height: CssPx(x_height),
        font_size: style.size,
    }
}
