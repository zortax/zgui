//! The result of one line-breaking pass.

use smallvec::SmallVec;

use crate::geometry::TextGeometry;
use crate::paragraph::inline_box::InlineBoxPlacement;

/// Where the lines fell, and where the atomic inlines landed.
///
/// Everything here is geometry: no glyphs, no styles, nothing a shaper owns. That is what lets a
/// layout engine consume it without naming a font engine, and what lets the same broken result be
/// compared against an independently computed one in a test.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrokenParagraph {
    /// The line boxes and the extent they fill.
    pub geometry: TextGeometry,
    /// Where each atomic inline's real top-left corner sits, relative to the paragraph's own.
    pub boxes: SmallVec<[InlineBoxPlacement; 2]>,
}
