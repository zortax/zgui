//! The seam a text engine is plugged in through.

use zgui_text_style::TextStyle;

use crate::geometry::strut::StrutMetrics;
use crate::paragraph::break_request::BreakRequest;
use crate::paragraph::broken::BrokenParagraph;
use crate::paragraph::content::ParagraphContent;
use crate::paragraph::shaped::ShapedParagraph;

/// Turns a paragraph into glyphs, and those glyphs into lines.
///
/// The two halves are separate methods because they cost two very different amounts, and every
/// caching decision in the pipeline rests on being able to do the second without the first.
///
/// # What an implementation must promise
///
/// * [`shape`](ParagraphShaper::shape) builds its result through
///   [`ShapedParagraph::new`], which is what makes a shape countable.
/// * [`break_lines`](ParagraphShaper::break_lines) asks
///   [`ShapedParagraph::begin_break`] first, and returns the previous result unchanged when it
///   answers `false`. That is the whole of the cheap path, and skipping the question turns every
///   width probe back into a full pass.
/// * Two calls with equal content produce equal glyphs, and two calls with equal requests against
///   the same shaped result produce equal lines. Without that, nothing downstream may cache.
pub trait ParagraphShaper {
    /// The shaper's own shaped form, carried through [`ShapedParagraph`] and never interpreted
    /// outside the shaper.
    type Engine;

    /// Shapes one paragraph.
    fn shape(&mut self, content: &ParagraphContent<'_>) -> ShapedParagraph<Self::Engine>;

    /// Breaks an already shaped paragraph into lines at the requested width.
    fn break_lines(
        &mut self,
        shaped: &mut ShapedParagraph<Self::Engine>,
        request: &BreakRequest<'_>,
    ) -> BrokenParagraph;

    /// The strut of a block whose root text style is `style`.
    ///
    /// Separate from shaping because a block establishes a strut whether or not it holds any text,
    /// and an empty line is exactly as tall as it.
    fn strut(&mut self, style: &TextStyle) -> StrutMetrics;

    /// Visits the style-uniform runs of one already broken line, in the order they are drawn.
    ///
    /// This is the only route from a shaped result to positioned glyphs, and it is deliberately
    /// `&self`: reading where the glyphs are is not a write, and a shaper held mutably for the
    /// whole of painting could not also be measuring.
    ///
    /// The positions handed to `visit` are relative to the line box's own top-left corner, so a
    /// caller that knows where the line landed knows where every glyph landed without asking
    /// anything else. A line index past the last line visits nothing.
    fn visit_line(
        &self,
        shaped: &ShapedParagraph<Self::Engine>,
        line: u16,
        visit: &mut dyn FnMut(crate::glyph::run::ShapedRun<'_>),
    );

    /// Visits the direction-uniform cluster runs of one already broken line, in the order they are
    /// drawn.
    ///
    /// This is what a caret and a hit test are computed from, and it is a separate question from
    /// [`visit_line`](ParagraphShaper::visit_line): a glyph is what is painted, a cluster is what
    /// can be selected, and on a ligature or a mark the two do not correspond. A line index past
    /// the last line visits nothing.
    ///
    /// The clusters of a run cover its bytes exactly once and in ascending order, whichever
    /// direction the run is drawn in, and every offset is measured from the line box's own start
    /// edge — the same coordinate `visit_line` reports glyph positions in.
    fn visit_clusters(
        &self,
        shaped: &ShapedParagraph<Self::Engine>,
        line: u16,
        visit: &mut dyn FnMut(crate::cluster::ClusterRun<'_>),
    );
}
