//! The shaper's own shaped form.

use zgui_text::BrokenParagraph;

use crate::shape::brush::SlotBrush;

/// One paragraph's shaped glyphs, as the engine holds them.
///
/// Carried through a [`ShapedParagraph`](zgui_text::ShapedParagraph) and read at paint time. The
/// layout can be broken again in place any number of times without re-shaping, which is what the
/// whole caching arrangement above it exists to exploit.
#[derive(Debug)]
pub struct ShapedLayout {
    /// The glyphs, and whatever line breaking was last applied to them.
    pub layout: parley::Layout<SlotBrush>,
    /// How many bytes at the front of the string the engine shaped belong to the directional
    /// prefix.
    ///
    /// Offsets read straight out of [`layout`](ShapedLayout::layout) — line ranges, cluster ranges,
    /// the engine's own hit tests — count these bytes, and the caller's text and text map do not.
    /// Everything this crate reports subtracts it before handing an offset out, so this number is
    /// needed only by whoever reaches past that and asks the engine directly.
    pub prefix: usize,
    /// The lines the current break produced, kept so that a request the glyphs already reflect can
    /// be answered without breaking again.
    pub(crate) last: BrokenParagraph,
    /// Whether any run of the paragraph may wrap. A paragraph with none breaks only where its
    /// text forces a break.
    pub(crate) wraps: bool,
}

/// The size of the engine's layout record itself, without what it holds on the heap.
#[doc(hidden)]
pub const LAYOUT_SIZE: usize = core::mem::size_of::<parley::Layout<SlotBrush>>();

/// Approximate sizes of the per-item records parley 0.11 keeps in private vectors.
///
/// The records are private, so their sizes are estimated from their fields. The public metric
/// structs inside them are measured.
mod record {
    /// One cluster: its character, flags, glyph and text offsets, and its advance.
    pub(super) const CLUSTER: usize = 28;
    /// One shaped run, before its metrics: the font, its attributes and synthesis, and four ranges.
    pub(super) const RUN: usize = 128;
    /// One line, before its metrics: two ranges, the break reason, the advance and the indent.
    pub(super) const LINE: usize = 56;
    /// One item of a line: its kind, index, level, advance and two ranges.
    pub(super) const LINE_ITEM: usize = 56;
    /// One layout item: its kind, index and level.
    pub(super) const ITEM: usize = 24;
}

impl ShapedLayout {
    /// An estimate of the heap bytes the engine's layout holds.
    ///
    /// Computed from what the layout holds, so equal paragraphs broken at equal widths report
    /// equal bytes. The cost is one step per line and per run. A layout that has no lines yet
    /// reports its styles and inline boxes only, because its runs are reachable through its lines
    /// alone.
    ///
    /// The estimate is approximate. A run that wraps is counted once per line. Glyphs held out of
    /// line (ligatures, complex scripts), vector capacity and fonts are not counted.
    pub fn heap_bytes(&self) -> usize {
        let layout = &self.layout;
        let mut bytes =
            core::mem::size_of_val(layout.styles()) + core::mem::size_of_val(layout.inline_boxes());
        for line in layout.lines() {
            bytes += record::LINE + core::mem::size_of::<parley::LineMetrics>();
            for item in line.items() {
                bytes += record::LINE_ITEM + record::ITEM;
                let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                // A cluster with one plain glyph holds it inline, so the clusters alone are
                // counted. The cost of the estimate thus grows with the lines and runs only.
                bytes += record::RUN
                    + core::mem::size_of::<parley::RunMetrics>()
                    + core::mem::size_of_val(run.normalized_coords())
                    + run.len() * record::CLUSTER;
            }
        }
        bytes
    }
}
