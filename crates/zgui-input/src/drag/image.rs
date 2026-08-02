//! What follows the pointer while a drag is in progress.

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point, Rect, Size};

/// The thing drawn under the pointer while something is being dragged.
///
/// It is described rather than rendered here: an element to draw and where to hold it, which the
/// paint stage turns into pixels. Rasterising a snapshot at drag start would freeze the element as
/// it looked then, so a row that is still animating its press state would be dragged around in a
/// stale frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragImage {
    /// The element to draw.
    pub node: NodeKey,
    /// Where inside its own box the pointer is holding it, from its top-left corner.
    ///
    /// This is what makes a dragged row stay under the part of it that was grabbed instead of
    /// jumping so that its corner is at the pointer.
    pub grab: Size<CssPx, Css>,
}

impl DragImage {
    /// Holds `node` at the point inside `bounds` where the pointer pressed.
    pub fn grabbed(node: NodeKey, bounds: Rect<CssPx, Css>, at: Point<CssPx, Css>) -> Self {
        Self {
            node,
            grab: Size::new(
                CssPx(at.x.0 - bounds.origin.x.0),
                CssPx(at.y.0 - bounds.origin.y.0),
            ),
        }
    }

    /// Where the image's top-left corner goes when the pointer is at `at`.
    pub fn origin_for(&self, at: Point<CssPx, Css>) -> Point<CssPx, Css> {
        Point::new(
            CssPx(at.x.0 - self.grab.width.0),
            CssPx(at.y.0 - self.grab.height.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters};
    use zgui_geom::{Css, CssPx, Point, Rect, Size};
    use zgui_interned::ElementName;

    use super::DragImage;

    #[test]
    fn a_dragged_row_stays_under_the_part_of_it_that_was_grabbed() {
        let document = Document::new();
        let node = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                root
            })
            .expect("not poisoned");
        let key = document.store().key_of(node);

        let bounds = Rect::<CssPx, Css>::new(
            Point::new(CssPx(100.0), CssPx(200.0)),
            Size::new(CssPx(300.0), CssPx(40.0)),
        );
        let image = DragImage::grabbed(key, bounds, Point::new(CssPx(280.0), CssPx(220.0)));
        assert_eq!(image.grab, Size::new(CssPx(180.0), CssPx(20.0)));

        let moved = image.origin_for(Point::new(CssPx(280.0), CssPx(500.0)));
        assert_eq!(
            moved,
            Point::new(CssPx(100.0), CssPx(480.0)),
            "the row moved down 280 and did not jump sideways"
        );
    }
}
