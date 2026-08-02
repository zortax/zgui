//! Keeping the shaper's inline boxes in step with the geometry a caller last measured.

use zgui_text::InlineBoxGeometry;

use crate::shape::brush::SlotBrush;

/// Copies each atomic inline's current width and declared height into the layout.
///
/// This is the step a re-styled `vertical-align` reaches the output through, and it is the reason
/// the geometry is an input on every breaking pass rather than state the shaper remembers.
///
/// The engine places an inline box with its bottom edge on the baseline, so the only lever for
/// moving the box is the height it is told; the alignment shift is therefore folded into that
/// height. Which means nothing in the shaped glyphs can notice the shift changed — a paragraph
/// whose shaping is a cache hit would leave its boxes exactly where they were, and a `vertical-align`
/// re-style would be a silent no-op rather than a visible error. Pushing the current heights in
/// before every break is what makes it move.
pub(crate) fn push_geometry(layout: &mut parley::Layout<SlotBrush>, boxes: &[InlineBoxGeometry]) {
    if boxes.is_empty() {
        return;
    }
    for inline_box in layout.inline_boxes_mut() {
        let Some(geometry) = boxes.iter().find(|geometry| geometry.id == inline_box.id) else {
            continue;
        };
        inline_box.width = geometry.width.0;
        inline_box.height = geometry.shaper_height().0;
    }
}
