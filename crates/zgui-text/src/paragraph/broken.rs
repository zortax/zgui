//! The result of one line-breaking pass.

use std::sync::Arc;

use smallvec::SmallVec;

use crate::geometry::TextGeometry;
use crate::paragraph::inline_box::InlineBoxPlacement;

/// Where the lines fell, and where the atomic inlines landed.
///
/// Everything here is geometry: no glyphs, no styles, nothing a shaper owns. That is what lets a
/// layout engine consume it without naming a font engine, and what lets the same broken result be
/// compared against an independently computed one in a test.
///
/// The lines are shared rather than owned, because one breaking pass's answer lives in several
/// places at once — the engine's laid-out form, the recall ring, and whatever layout is holding —
/// and a paragraph re-broken on every step of a drag paid a copy of its whole line vector into
/// each of them. Nothing mutates a broken result; a new pass makes a new one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrokenParagraph {
    /// The line boxes and the extent they fill.
    pub geometry: Arc<TextGeometry>,
    /// Where each atomic inline's real top-left corner sits, relative to the paragraph's own.
    pub boxes: SmallVec<[InlineBoxPlacement; 2]>,
}
