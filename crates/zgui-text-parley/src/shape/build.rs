//! Turning one paragraph's content into glyphs.

use zgui_geom::CssPx;
use zgui_text::{
    ContentWidths, InlineBoxGeometry, ParagraphContent, ParagraphKey, ShapedParagraph,
    StrutMetrics, StyledRun,
};
use zgui_text_style::TextStyle;

use crate::direction::Controls;
use crate::shape::brush::SlotBrush;
use crate::shape::engine::ShapedLayout;
use crate::shape::style::LoweredStyle;

/// Shapes one paragraph, prefixing the control that forces its base direction.
///
/// The prefix goes in here rather than into the string a caller generated, because it is a
/// property of how this engine is driven and not of the document. It is therefore taken back off
/// again on the way out: the shaped result carries the caller's own string and the caller's own
/// map, and every offset read out of the layout afterwards — [line ranges](crate::shape::lines),
/// [cluster ranges](crate::shape::clusters) — has the prefix subtracted before it is reported.
///
/// So there is one byte space on this boundary rather than two. A shifted map beside offsets that
/// counted the prefix would be self-consistent and wrong in the same measure: mapping an offset out
/// and back would round-trip, while a caret placed from a cluster and a click resolved to one would
/// land a prefix apart from each other on the screen.
pub(crate) fn shape(
    content: &ParagraphContent<'_>,
    strut: StrutMetrics,
    controls: Controls,
    fonts: &mut parley::FontContext,
    scratch: &mut parley::LayoutContext<SlotBrush>,
) -> ShapedParagraph<ShapedLayout> {
    let prefix = controls.prefix(content.paragraph.direction);
    let mut text = String::with_capacity(prefix.len() + content.text.len());
    text.push_str(prefix);
    text.push_str(content.text);

    let mut builder = scratch.ranged_builder(fonts, &text, content.scale, true);
    default_style(content).push_default(&mut builder);
    for run in content.runs {
        let range = run.text.start + prefix.len()..run.text.end + prefix.len();
        if range.is_empty() {
            continue;
        }
        LoweredStyle::of(&run.style, run.brush, CssPx::ZERO).push_over(&mut builder, range);
    }
    for geometry in content.boxes {
        builder.push_inline_box(inline_box(geometry, prefix.len()));
    }

    let mut layout: parley::Layout<SlotBrush> = builder.build(&text);
    let widths = layout.calculate_content_widths();
    // A first break with no width constraint is what produces line metrics at all; every later
    // request re-breaks the same glyphs.
    layout.break_all_lines(None);
    let has_boxes = !content.boxes.is_empty();
    let engine = ShapedLayout {
        last: crate::shape::lines::read(&layout, has_boxes, prefix.len()),
        layout,
        prefix: prefix.len(),
    };
    ShapedParagraph::new(
        ParagraphKey::of(content),
        content.text.to_owned(),
        content.map.clone(),
        ContentWidths {
            min: CssPx(widths.min),
            max: CssPx(widths.max),
        },
        strut,
        content.boxes.iter().copied(),
        engine,
    )
}

/// The style the paragraph's own defaults come from.
///
/// The first run's, because a paragraph's runs are what it is made of and the first one is the
/// style anything outside every run — the directional prefix, and a paragraph with no runs at all
/// — is measured in. A paragraph with no runs falls back to the initial style, which is the same
/// answer an empty document would give.
pub(crate) fn default_style(content: &ParagraphContent<'_>) -> LoweredStyle {
    match content.runs.first() {
        Some(StyledRun { style, brush, .. }) => LoweredStyle::of(style, *brush, CssPx::ZERO),
        None => LoweredStyle::of(&TextStyle::initial(), zgui_scene::PaintSlot(0), CssPx::ZERO),
    }
}

/// One atomic inline, at the width and the declared height the caller measured it with.
fn inline_box(geometry: &InlineBoxGeometry, prefix: usize) -> parley::InlineBox {
    parley::InlineBox {
        id: geometry.id,
        kind: parley::InlineBoxKind::InFlow,
        index: geometry.offset + prefix,
        width: geometry.width.0,
        height: geometry.shaper_height().0,
    }
}
