//! What a broken paragraph occupies, in the units a layout engine speaks.

pub mod line;
pub mod strut;

pub use crate::geometry::line::LineGeometry;
pub use crate::geometry::strut::StrutMetrics;

use zgui_geom::{Css, CssPx, Size};

/// The lines of a broken paragraph and the box they fill.
///
/// This is the whole of what a layout engine needs back from a text leaf: a size to report, and the
/// two baselines a surrounding flex or grid line aligns against. Everything glyph-shaped stays
/// inside the shaper's own result and is read at paint time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextGeometry {
    /// The lines, in visual top-to-bottom order.
    pub lines: Vec<LineGeometry>,
    /// The box the lines fill.
    pub size: Size<CssPx, Css>,
    /// Whether the paragraph's base direction came out right-to-left.
    pub is_rtl: bool,
}

impl TextGeometry {
    /// The baseline a parent aligns this paragraph's first line against.
    ///
    /// Absent when the paragraph has no lines at all, which an empty inline formatting context has
    /// — and which is why this is optional rather than zero, since a parent must then fall back to
    /// the paragraph's own bottom edge rather than aligning to its top.
    pub fn first_baseline(&self) -> Option<CssPx> {
        self.lines.first().map(|line| line.baseline)
    }

    /// The baseline a parent aligns this paragraph's last line against.
    pub fn last_baseline(&self) -> Option<CssPx> {
        self.lines.last().map(|line| line.baseline)
    }

    /// The height every line together occupies.
    pub fn height(&self) -> CssPx {
        self.lines
            .last()
            .map_or(CssPx::ZERO, |line| CssPx(line.top.0 + line.height.0))
    }
}
