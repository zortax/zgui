//! What a line draws besides its glyphs, as a rectangle the cull can be asked about.
//!
//! A fragment is skipped when the frame's damage does not reach its ink, and a line's ink is where
//! its glyphs are. That is the whole of the answer for every line of every document — except the
//! one a person has just emptied. Such a line holds no glyph and its ink is a rectangle of no
//! width, which no damage can intersect; the caret drawn on it is a rectangle of its own, damaged
//! by the stage that planned it, and the fragment carrying it is skipped before anything asks what
//! it would have drawn. The field then has no insertion point in it until something else happens to
//! damage the same pixels.
//!
//! So the cull asks the caret's owner where it is going to draw, in the same coordinates and
//! through the same call the emission will use — not a second opinion about where a caret goes, but
//! the same one, asked earlier.

use zgui_geom::{Device, DevicePx, Rect};
use zgui_layout::{Fragment, FragmentKind};
use zgui_scene::Placements;

use crate::emit::highlight::{HighlightRequest, HighlightSource};

/// The rectangle covering everything `highlights` would draw with `fragment`.
///
/// `None` for a fragment that is not a line, and for a line with no caret and nothing selected on
/// it — which is every line of every document nobody is typing into, so the cull pays one virtual
/// call and no allocation for the overwhelming majority of fragments.
///
/// `scale` is how many device pixels one CSS pixel is, and it is the frame's own: it decides how
/// wide the caret is, and a rectangle measured at another scale is not the one that will be drawn.
///
/// The marks are measured against the line box's own corner, which is the fragment's space before
/// its transform — but the damage this rectangle is tested against is measured on the device. The
/// emission puts the same marks through the fragment's matrix before drawing them, so the cull has
/// to take the same step or it answers about the wrong pixels: on a turned paragraph the
/// untransformed rectangle lands over *other* lines, whose fragments then repaint — or fail to —
/// for a selection that was never on them. A caller with no placements to resolve the matrix
/// through gets the untransformed rectangle, which is exact for everything upright.
pub fn extent(
    fragment: &Fragment,
    highlights: &dyn HighlightSource,
    scale: f32,
    placements: Option<&Placements>,
) -> Option<Rect<DevicePx, Device>> {
    let FragmentKind::Line { paragraph, line } = fragment.kind else {
        return None;
    };
    // The line box's own corner, which is the origin the emission measures its marks against.
    let request = HighlightRequest {
        origin: fragment.border_box.origin,
        scale,
    };
    let mut held: Option<Rect<DevicePx, Device>> = None;
    highlights.visit_line(paragraph, line, request, &mut |highlight| {
        held = Some(match held {
            Some(union) => union.union(highlight.bounds),
            None => highlight.bounds,
        });
    });
    match placements {
        Some(placements) => held.map(|rect| {
            zgui_layout::fragment::transform::placed::onto_device(
                rect,
                fragment.transform,
                placements,
            )
        }),
        None => held,
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{DevicePx, Point, Rect, Size};
    use zgui_layout::fragment::ParagraphId;
    use zgui_layout::{Fragment, FragmentKind};

    use super::extent;
    use crate::emit::highlight::{
        Highlight, HighlightLayer, HighlightRequest, HighlightSource, NoHighlights,
    };

    /// A minted key, for a test that needs a name and not a stored value.
    fn key<T>(index: u32) -> zgui_arena::Key<T> {
        zgui_arena::Key::new(
            index,
            zgui_arena::Generation::new(1).expect("one is a generation"),
            zgui_arena::DomainId::FIRST,
        )
    }

    /// A line fragment whose box is at `origin` and holds nothing, which is what an emptied field
    /// produces.
    fn blank_line(x: f32, y: f32) -> Fragment {
        let mut fragment = Fragment::new(
            key(0),
            key(0),
            FragmentKind::Line {
                paragraph: ParagraphId(0),
                line: 0,
            },
        );
        fragment.border_box = Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(0.0), DevicePx(24.0)),
        );
        fragment.ink = fragment.border_box;
        fragment
    }

    /// A source that draws one caret at the line box's own corner.
    struct OneCaret;

    impl HighlightSource for OneCaret {
        fn fingerprint(&self, _paragraph: ParagraphId, _line: u16) -> u64 {
            1
        }

        fn visit_line(
            &self,
            _paragraph: ParagraphId,
            _line: u16,
            request: HighlightRequest,
            visit: &mut dyn FnMut(Highlight),
        ) {
            visit(Highlight {
                bounds: Rect::new(request.origin, Size::new(DevicePx(1.0), DevicePx(24.0))),
                color: zgui_color::Color::BLACK,
                layer: HighlightLayer::InFront,
            });
        }
    }

    #[test]
    fn a_line_with_nothing_drawn_on_it_reports_nothing() {
        assert_eq!(extent(&blank_line(20.0, 12.0), &NoHighlights, 1.0, None), None);
    }

    /// The defect: a line box of no width, and a caret on it that has to survive the cull.
    #[test]
    fn a_caret_on_a_line_of_no_width_is_a_rectangle_with_area() {
        let found =
            extent(&blank_line(20.0, 12.0), &OneCaret, 1.0, None).expect("the caret is drawn");
        assert_eq!(found.origin.x.0, 20.0);
        assert_eq!(found.origin.y.0, 12.0);
        assert_eq!(found.size.width.0, 1.0);
        assert_eq!(found.size.height.0, 24.0);
    }

    #[test]
    fn a_fragment_that_is_not_a_line_is_never_asked() {
        let fragment = Fragment::new(key(0), key(0), FragmentKind::Box);
        assert_eq!(extent(&fragment, &OneCaret, 1.0, None), None);
    }

    /// The defect behind a selection on a turned paragraph erasing the lines above it: the marks
    /// are measured in the line's own space, and a cull that reads them untransformed tests the
    /// damage against pixels the drawing never touches.
    #[test]
    fn a_mark_on_a_transformed_line_is_read_where_it_is_drawn() {
        use zgui_geom::Matrix4;
        use zgui_scene::{OwnSpace, Placements, PropertyOwner, SpatialTree};

        let mut tree = SpatialTree::with_viewport();
        let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
        let moved = Matrix4::translation(100.0, 200.0, 0.0);
        let space = tree.space_of(tree.viewport(), owner, OwnSpace::of(Some(moved), None, false));
        let placements = Placements::of(&tree);

        let mut fragment = blank_line(20.0, 12.0);
        fragment.transform = Some(space);
        let found =
            extent(&fragment, &OneCaret, 1.0, Some(&placements)).expect("the caret is drawn");
        assert_eq!(found.origin.x.0, 120.0);
        assert_eq!(found.origin.y.0, 212.0);
    }
}
