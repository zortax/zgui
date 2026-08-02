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
}
