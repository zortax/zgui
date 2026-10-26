//! The shaper's own shaped form.

use smallvec::SmallVec;
use zgui_text::BrokenParagraph;

use crate::shape::brush::SlotBrush;
use crate::shape::frozen::Frozen;
use crate::shape::reach::Reach;

/// One paragraph's shaped glyphs, as the engine holds them.
///
/// Carried through a [`ShapedParagraph`](zgui_text::ShapedParagraph) and read at paint time. The
/// layout can be broken again in place any number of times without re-shaping, which is what the
/// whole caching arrangement above it exists to exploit.
///
/// A paragraph that can break one way only holds a compact frozen form in place of the engine's
/// layout. See [`Shaper::freezing`](crate::Shaper::freezing) for the conditions.
#[derive(Debug)]
pub struct ShapedLayout {
    /// How many bytes at the front of the string the engine shaped belong to the directional
    /// prefix.
    ///
    /// Offsets read straight out of the [`layout`](ShapedLayout::layout) — line ranges, cluster
    /// ranges, the engine's own hit tests — count these bytes, and the caller's text and text map
    /// do not. Everything this crate reports subtracts it before handing an offset out, so this
    /// number is needed only by whoever reaches past that and asks the engine directly.
    pub prefix: usize,
    /// The lines the current break produced, kept so that a request the glyphs already reflect can
    /// be answered without breaking again.
    pub(crate) last: BrokenParagraph,
    /// The glyphs, in one of two forms.
    pub(crate) form: Form,
}

/// The two forms a shaped paragraph is held in.
#[derive(Debug)]
pub(crate) enum Form {
    /// The engine's layout, which breaks again at any request.
    Live(Box<Live>),
    /// The glyphs and clusters of the one break the paragraph has, without the engine's layout.
    Frozen(Frozen),
}

/// The engine's layout, and what the next break reads beside it.
#[derive(Debug)]
pub(crate) struct Live {
    /// The glyphs, and whatever line breaking was last applied to them.
    pub(crate) layout: parley::Layout<SlotBrush>,
    /// How many items each line of the current break holds, when the next break may keep
    /// [`last`](ShapedLayout::last).
    ///
    /// Empty after a banded or justified pass. A break always gives at least one line. See
    /// [`lines::unchanged`](crate::shape::lines::unchanged).
    pub(crate) items: SmallVec<[u32; 4]>,
    /// Whether any run of the paragraph may wrap. A paragraph with none breaks only where its
    /// text forces a break.
    pub(crate) wraps: bool,
    /// Whether a breaking pass that [`reach`](crate::shape::reach) covers ran before.
    pub(crate) rebroken: bool,
    /// The widths over which the current break keeps its lines.
    pub(crate) reach: Reach,
}

impl ShapedLayout {
    /// The engine's layout, with whatever line breaking was last applied to it.
    ///
    /// `None` when the paragraph holds the frozen form.
    pub fn layout(&self) -> Option<&parley::Layout<SlotBrush>> {
        match &self.form {
            Form::Live(live) => Some(&live.layout),
            Form::Frozen(_) => None,
        }
    }

    /// Whether the paragraph holds the frozen form in place of the engine's layout.
    pub fn is_frozen(&self) -> bool {
        matches!(self.form, Form::Frozen(_))
    }
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
    /// equal bytes. The cost grows with the glyphs, because the engine's line items walk them. A
    /// layout that has no lines yet reports its styles and inline boxes only, because its runs are
    /// reachable through its lines alone.
    ///
    /// The step table that lets a pass skip the engine's breaker is counted as well. A paragraph
    /// holds one from its second breaking pass on, so two equal paragraphs can differ by it.
    ///
    /// The estimate is approximate. A run that wraps is counted once per line. Glyphs held out of
    /// line (ligatures, complex scripts), vector capacity and fonts are not counted.
    ///
    /// The bytes of a frozen form are counted exactly.
    pub fn heap_bytes(&self) -> usize {
        match &self.form {
            Form::Live(live) => live.heap_bytes(),
            Form::Frozen(frozen) => frozen.heap_bytes(),
        }
    }
}

impl Live {
    /// An estimate of the heap bytes the live form holds; see [`ShapedLayout::heap_bytes`].
    fn heap_bytes(&self) -> usize {
        let layout = &self.layout;
        let mut bytes = core::mem::size_of::<Live>()
            + core::mem::size_of_val(layout.styles())
            + core::mem::size_of_val(layout.inline_boxes())
            + self.reach.heap_bytes();
        for line in layout.lines() {
            bytes += record::LINE + core::mem::size_of::<parley::LineMetrics>();
            for item in line.items() {
                bytes += record::LINE_ITEM + record::ITEM;
                let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                // A cluster with one plain glyph holds it inline, so the clusters alone are
                // counted.
                bytes += record::RUN
                    + core::mem::size_of::<parley::RunMetrics>()
                    + core::mem::size_of_val(run.normalized_coords())
                    + run.len() * record::CLUSTER;
            }
        }
        bytes
    }
}
