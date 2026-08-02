//! What one line-breaking pass is asked for.

use zgui_geom::CssPx;
use zgui_text_style::{BreakingKey, ParagraphStyle};

use crate::paragraph::band::LineBands;
use crate::paragraph::content::{ParagraphContent, StyledRun};
use crate::paragraph::inline_box::InlineBoxGeometry;
use crate::paragraph::key::breaking_key;

/// Everything a breaking pass reads that is not already in the shaped result.
///
/// A layout engine calls this many times per paragraph while it resolves the surrounding flex or
/// grid, each time at a different width, and each call must cost a break rather than a shape.
///
/// The atomic inlines' geometry is passed in *every* time rather than remembered, and that is the
/// point rather than an inconvenience: an inline box can have resized under a different constraint,
/// and its `vertical-align` can have been re-styled, and neither of those is visible to a shaper —
/// so a request that let a caller omit them would make a `vertical-align` change against a warm
/// cache a silent no-op.
#[derive(Clone, Copy, Debug)]
pub struct BreakRequest<'a> {
    /// The runs, which contribute their breaking-side properties.
    pub runs: &'a [StyledRun],
    /// The atomic inlines, at the geometry they currently have.
    pub boxes: &'a [InlineBoxGeometry],
    /// The paragraph's own alignment and indent.
    pub paragraph: &'a ParagraphStyle,
    /// The width to break into, or nothing for "as wide as it likes".
    pub max_advance: Option<CssPx>,
    /// The width a percentage indent is measured against, when the paragraph has one.
    ///
    /// Separate from [`max_advance`](BreakRequest::max_advance) because an intrinsic probe has no
    /// width to break into and still resolves an indent against the containing block.
    pub indent_basis: Option<CssPx>,
    /// The width each individual line may take, when the lines do not all share one.
    ///
    /// Empty for a paragraph with nothing floated beside it, which is the ordinary case.
    pub bands: LineBands<'a>,
    /// Whether the answer is one of several taken to find a size, rather than the one to be kept.
    ///
    /// A probe may be answered from a pass already taken at this width without the shaper's own
    /// laid-out form being moved to it; the kept answer may not, because that form is what its
    /// glyphs are drawn from. See [`Plan`](crate::Plan).
    pub probe: bool,
}

impl<'a> BreakRequest<'a> {
    /// A request breaking `content`'s runs and boxes at one width.
    pub fn new(content: &ParagraphContent<'a>, max_advance: Option<CssPx>) -> Self {
        Self {
            runs: content.runs,
            boxes: content.boxes,
            paragraph: content.paragraph,
            max_advance,
            indent_basis: max_advance,
            bands: LineBands::NONE,
            probe: false,
        }
    }

    /// The same request, with each line confined to its own band.
    #[must_use]
    pub fn banded(mut self, bands: LineBands<'a>) -> Self {
        self.bands = bands;
        self
    }

    /// The same request, marked as one of several taken to find a size.
    #[must_use]
    pub fn probing(mut self) -> Self {
        self.probe = true;
        self
    }

    /// The key this request breaks under.
    pub fn key(&self) -> BreakingKey {
        breaking_key(self)
    }

    /// The indent this request's paragraph resolves to.
    pub fn indent(&self) -> CssPx {
        self.paragraph
            .indent
            .length
            .resolve(self.indent_basis.unwrap_or(CssPx::ZERO))
    }
}
