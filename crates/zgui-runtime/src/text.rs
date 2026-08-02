//! The text engine seam, and the brush table a colour change rewrites.
//!
//! Layout asks the text engine to shape and break paragraphs; painting asks the same engine where
//! the glyphs of one line ended up. Neither of those needs anything from here — they are
//! [`MeasureContent`] and [`ShapedGlyphs`]. What the *frame* additionally needs is the brush
//! table, because a shaped paragraph stores a slot rather than a colour. Rewriting the slot re-colours every paragraph that inherited that colour, with nothing
//! re-shaped and no cache thrown away; and that rewrite has to happen in the same frame the
//! cascade moved the colour, or a theme change is visible one frame late on every string on the
//! screen.

use zgui_layout::MeasureContent;
use zgui_scene::TextPaintTable;
use zgui_text::{ClusterRun, ParagraphKey, ShapedClusters, ShapedGlyphs, ShapedRun};

/// What a text engine is holding shaped, and how often it has been read.
///
/// Two numbers rather than one because they answer different halves of the same budget question:
/// the first is what the cache costs, the second is whether anything is still getting value out of
/// it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShapedHeld {
    /// How many paragraphs are shaped right now.
    pub paragraphs: usize,
    /// How many lookups have found a shaped result since the engine was built.
    pub hits: u64,
}

impl ShapedHeld {
    /// An engine holding nothing and never read.
    pub const NOTHING: Self = Self {
        paragraphs: 0,
        hits: 0,
    };
}

/// A text engine, as the frame loop needs it.
///
/// Three seams in one, because a frame needs all three of the same engine and holding three
/// objects that had to agree about which paragraphs exist would be three chances to disagree:
/// measuring, for layout; positioned glyphs, for painting; and the brush table, which is the one
/// a measurer cannot be asked for. The frame writes through the brushes after the cascade has
/// settled and before anything is painted.
pub trait TextEngine: MeasureContent + ShapedGlyphs + ShapedClusters {
    /// The brushes runs are drawn with.
    fn text_paints(&mut self) -> &mut TextPaintTable;

    /// How many paragraphs are shaped right now, and how often the shaping has been read.
    ///
    /// The hit count is monotonic and never reset, because what a budget asks is whether anything
    /// read the cache *between* two moments; a per-frame figure would need the engine to be told
    /// when a frame is.
    fn shaped_held(&self) -> ShapedHeld;

    /// Drops every shaped paragraph, and reports how many that threw away.
    ///
    /// The expensive escape hatch, for the cases where what a cached shaping *records* has stopped
    /// being true — a font finishing loading, or an element's runs having to be re-brushed because
    /// the slot they name can no longer be rewritten on their behalf.
    ///
    /// The count is not decoration. Everything measured from a dropped paragraph is stale, and a
    /// caller has to invalidate exactly as much as was dropped: nothing, for a window holding no
    /// text at all, and every box that was measured, for one that was.
    fn forget_shaped(&mut self) -> usize;

    /// Drops the shaped paragraphs held under `keys`, and reports how many that threw away.
    ///
    /// The narrow form of [`TextEngine::forget_shaped`], and the one the frame reaches for: a run
    /// whose brush slot can no longer be rewritten on its behalf makes the shaping of the
    /// paragraphs *that run is in* wrong, and says nothing about any other string in the window.
    /// The count carries the same obligation, over the same paragraphs.
    fn forget_paragraphs(&mut self, keys: &[ParagraphKey]) -> usize;
}

impl<S, R> TextEngine for zgui_layout::Paragraphs<S, R>
where
    S: zgui_text::ParagraphShaper,
    R: MeasureContent,
{
    fn text_paints(&mut self) -> &mut TextPaintTable {
        self.paints_mut()
    }

    fn shaped_held(&self) -> ShapedHeld {
        let cache = self.cache();
        ShapedHeld {
            paragraphs: cache.len(),
            hits: cache.hits(),
        }
    }

    fn forget_shaped(&mut self) -> usize {
        zgui_layout::Paragraphs::forget_shaped(self)
    }

    fn forget_paragraphs(&mut self, keys: &[ParagraphKey]) -> usize {
        zgui_layout::Paragraphs::forget_paragraphs(self, keys)
    }
}

impl ShapedGlyphs for Box<dyn TextEngine> {
    fn visit_line(&self, paragraph: ParagraphKey, line: u16, visit: &mut dyn FnMut(ShapedRun<'_>)) {
        (**self).visit_line(paragraph, line, visit);
    }
}

impl ShapedClusters for Box<dyn TextEngine> {
    fn visit_clusters(
        &self,
        paragraph: ParagraphKey,
        line: u16,
        visit: &mut dyn FnMut(ClusterRun<'_>),
    ) {
        (**self).visit_clusters(paragraph, line, visit);
    }
}

impl MeasureContent for Box<dyn TextEngine> {
    fn measure(&mut self, request: zgui_layout::MeasureRequest<'_>) -> zgui_layout::Measured {
        (**self).measure(request)
    }

    fn shape(
        &mut self,
        content: &zgui_text::ParagraphContent<'_>,
    ) -> zgui_layout::measure::ShapedSummary {
        (**self).shape(content)
    }

    fn break_lines(
        &mut self,
        key: zgui_text::ParagraphKey,
        request: &zgui_text::BreakRequest<'_>,
    ) -> zgui_text::BrokenParagraph {
        (**self).break_lines(key, request)
    }

    fn strut(&mut self, style: &zgui_text_style::TextStyle) -> zgui_text::StrutMetrics {
        (**self).strut(style)
    }

    fn paint_slot(&mut self, paint: &zgui_text_style::TextPaint) -> zgui_text::Brush {
        (**self).paint_slot(paint)
    }
}

impl TextEngine for Box<dyn TextEngine> {
    fn text_paints(&mut self) -> &mut TextPaintTable {
        (**self).text_paints()
    }

    fn shaped_held(&self) -> ShapedHeld {
        (**self).shaped_held()
    }

    fn forget_shaped(&mut self) -> usize {
        (**self).forget_shaped()
    }

    fn forget_paragraphs(&mut self, keys: &[ParagraphKey]) -> usize {
        (**self).forget_paragraphs(keys)
    }
}

/// A text engine that shapes nothing.
///
/// For an application whose window holds no text at all, and for bringing a window up before a
/// font engine has been chosen. It measures every paragraph to nothing, which is the honest answer
/// from something that cannot shape: a fixture that reported a plausible width would produce
/// layouts nothing else agrees with.
#[derive(Debug, Default)]
pub struct NoText {
    /// The brushes, so that the colour path still has somewhere to write.
    paints: TextPaintTable,
}

impl NoText {
    /// A text engine that shapes nothing.
    pub fn new() -> Self {
        Self::default()
    }
}

impl MeasureContent for NoText {
    fn measure(&mut self, request: zgui_layout::MeasureRequest<'_>) -> zgui_layout::Measured {
        zgui_layout::NoContent.measure(request)
    }

    fn shape(
        &mut self,
        content: &zgui_text::ParagraphContent<'_>,
    ) -> zgui_layout::measure::ShapedSummary {
        zgui_layout::NoContent.shape(content)
    }

    fn break_lines(
        &mut self,
        key: zgui_text::ParagraphKey,
        request: &zgui_text::BreakRequest<'_>,
    ) -> zgui_text::BrokenParagraph {
        zgui_layout::NoContent.break_lines(key, request)
    }

    fn strut(&mut self, style: &zgui_text_style::TextStyle) -> zgui_text::StrutMetrics {
        zgui_layout::NoContent.strut(style)
    }

    fn paint_slot(&mut self, paint: &zgui_text_style::TextPaint) -> zgui_text::Brush {
        let colour = paint.color;
        self.paints.slot_for(paint.key.addr() as u64, || {
            zgui_scene::TextPaint::new(colour)
        })
    }
}

impl ShapedClusters for NoText {
    fn visit_clusters(
        &self,
        _paragraph: ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(ClusterRun<'_>),
    ) {
    }
}

impl ShapedGlyphs for NoText {
    fn visit_line(
        &self,
        _paragraph: ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(ShapedRun<'_>),
    ) {
    }
}

impl TextEngine for NoText {
    fn text_paints(&mut self) -> &mut TextPaintTable {
        &mut self.paints
    }

    fn shaped_held(&self) -> ShapedHeld {
        ShapedHeld::NOTHING
    }

    fn forget_shaped(&mut self) -> usize {
        0
    }

    fn forget_paragraphs(&mut self, _keys: &[ParagraphKey]) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{NoText, TextEngine};

    #[test]
    fn a_text_engine_is_usable_behind_a_pointer() {
        let mut engine: Box<dyn TextEngine> = Box::new(NoText::new());
        assert!(engine.text_paints().is_empty());
    }
}
