//! The horizontal space one line of a paragraph is allowed to occupy.

use zgui_geom::CssPx;

/// The strip of a paragraph's width that one line may use.
///
/// Lines of a paragraph normally all share the paragraph's width. They stop sharing it as soon as
/// something is floated beside the text: the lines beside the float are narrower and start further
/// in, and the lines below it are the full width again. So the width a line breaks into is a
/// property of *that line*, not of the paragraph, whenever any band is supplied.
///
/// A band is expressed in the paragraph's own coordinates, so
/// [`offset`](LineBand::offset) is measured from the paragraph's start edge and
/// [`max_advance`](LineBand::max_advance) is what remains after both insets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineBand {
    /// How far in from the paragraph's start edge the line begins.
    pub offset: CssPx,
    /// How wide the line may be.
    pub max_advance: CssPx,
}

impl LineBand {
    /// A band occupying the whole of `width`.
    pub fn full(width: CssPx) -> Self {
        Self {
            offset: CssPx::ZERO,
            max_advance: width,
        }
    }
}

/// The band each line of a paragraph breaks into.
///
/// An empty list means every line takes the request's own width, which is what a paragraph with
/// nothing floated beside it wants and costs nothing to express. A non-empty list gives line `i`
/// the band at index `i`, and every line past the end of the list the last band — because floats
/// end somewhere, and the lines below the lowest one all share the width that is left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LineBands<'a> {
    /// The bands, in line order.
    bands: &'a [LineBand],
}

impl<'a> LineBands<'a> {
    /// No banding: every line takes the width it was asked to break into.
    pub const NONE: Self = Self { bands: &[] };

    /// The bands for the first lines, with the last repeating for the rest.
    ///
    /// ```
    /// use zgui_geom::CssPx;
    /// use zgui_text::{LineBand, LineBands};
    ///
    /// let narrow = LineBand { offset: CssPx(60.0), max_advance: CssPx(140.0) };
    /// let wide = LineBand::full(CssPx(200.0));
    /// let bands = [narrow, wide];
    /// let bands = LineBands::new(&bands);
    ///
    /// assert_eq!(bands.at(0), Some(narrow));
    /// assert_eq!(bands.at(1), Some(wide));
    /// assert_eq!(bands.at(9), Some(wide), "the last band covers everything below it");
    /// assert_eq!(LineBands::NONE.at(0), None);
    /// ```
    pub fn new(bands: &'a [LineBand]) -> Self {
        Self { bands }
    }

    /// The band line `index` breaks into, or nothing when no banding was supplied.
    pub fn at(&self, index: usize) -> Option<LineBand> {
        if self.bands.is_empty() {
            return None;
        }
        self.bands.get(index).or_else(|| self.bands.last()).copied()
    }

    /// Whether any band was supplied at all.
    pub fn is_empty(&self) -> bool {
        self.bands.is_empty()
    }

    /// The bands as supplied.
    pub fn as_slice(&self) -> &'a [LineBand] {
        self.bands
    }
}
