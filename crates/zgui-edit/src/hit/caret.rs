//! Where the caret is drawn.

use zgui_geom::{Css, CssPx, Point, Size};

/// The caret's box, in the paragraph's own coordinates.
///
/// A caret is as tall as the line box it sits on rather than as tall as the text on it, which is
/// what keeps it from shrinking beside a superscript and what the platform wants when it is told
/// where to put a candidate window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    /// The top-left corner, measured from the paragraph's top-left corner.
    pub origin: Point<CssPx, Css>,
    /// How tall the caret is; its width is the caller's, because that is a style decision.
    pub height: CssPx,
    /// Which line it landed on.
    pub line: usize,
}

impl Caret {
    /// The caret's box at a given width, which is what a platform is told about.
    pub fn rect(&self, width: CssPx) -> (Point<CssPx, Css>, Size<CssPx, Css>) {
        (self.origin, Size::new(width, self.height))
    }
}
