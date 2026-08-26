//! What a box clips its descendants to.
//!
//! A clip is a chain and not a rectangle: a rounded card inside a scrollport inside another
//! scrollport is three tests, and every one of them has to be applied. So a box that clips adds one
//! link to whatever chain it was drawn under, and the chain a descendant carries is the whole
//! ancestry rather than the nearest clipper.
//!
//! Two things do *not* clip and are handled by not being here. `overflow: visible` adds no link at
//! all, which is the overwhelming majority of boxes; and a box's own border box is never clipped by
//! itself, only its descendants are — a shadow spreading outside a scrollport belongs to the
//! scrollport, not to its contents.

use zgui_css::ComputedStyle;
use zgui_css::values::border::BorderCornerRadiusValue;
use zgui_css::values::length::{LengthPercentage, evaluate_at};
use zgui_css::values::size::OverflowValue;
use zgui_geom::{Corners, CssPx, Device, DevicePx, Edges, Rect, Size, Vec2};
use zgui_scene::{ClipId, ClipLink, ClipTable};

/// Whether a box clips what is inside it.
///
/// `visible` clips nothing. Every other value does, including `auto`, whose scrollport is a clip
/// whether or not a scrollbar is showing.
pub fn clips_children(style: &ComputedStyle) -> bool {
    let box_ = style.get_box();
    box_.overflow_x != OverflowValue::Visible || box_.overflow_y != OverflowValue::Visible
}

/// The chain descendants of this box are drawn under.
///
/// The clip is the *padding* box, because a scrollport's content is clipped inside the border and
/// not inside the padding, and it takes the box's own corner radii so that content is cut to the
/// curve rather than to the corner it is inscribed in.
///
/// `owner` names the box, and the name is the chain's identity: the same box under the same parent
/// chain is the same chain whatever this frame's layout gave it, so a resize rewrites the stored
/// rectangle in place and every record naming the chain keeps naming it.
///
/// `shift` is everything the scroll and sticky offsets above this box have added to where it is
/// drawn. It travels with the link so that a residual chain derived from this one — which is named
/// by settled geometry rather than by a box — holds still while a scroll runs.
///
/// `space` is the coordinate system the padding box is measured in — the one this box's own
/// fragments draw under. It travels with the link because the rectangle is only device pixels when
/// nothing above the box is transformed: a field inside a dialog held off-centre by its placement
/// measures its clip where the field was laid out, and everything that applies the clip asks where
/// the field is drawn. The link carries the name and whoever applies it resolves the matrix, so a
/// box moved by writing its coordinate system — which never re-interns this chain — still clips
/// where it is, not where it was.
#[expect(
    clippy::too_many_arguments,
    reason = "each one is a separate measurement of the box, and the three the doc above explains \
              are explained one at a time; a struct holding them would be a bag with one caller \
              shape and would take the argument for `shift` and `space` out of the signature"
)]
pub fn chain_for_children(
    clips: &mut ClipTable,
    parent: ClipId,
    style: &ComputedStyle,
    padding_box: Rect<DevicePx, Device>,
    border: Edges<DevicePx>,
    scale: f32,
    shift: Size<DevicePx, Device>,
    space: zgui_scene::SpatialId,
    owner: zgui_scene::PropertyOwner,
) -> ClipId {
    if !clips_children(style) {
        return parent;
    }
    let radii = inner_radii(style, padding_box, border, scale);
    clips.push_named(
        parent,
        ClipLink::RoundedRect {
            rect: padding_box,
            radii,
            space,
        },
        shift,
        owner,
    )
}

/// A box's four corner radii, in device pixels, resolved against its own border box.
///
/// A percentage radius is a percentage of the box's own extent on that axis, and the two radii of
/// one corner may differ, so a corner is an ellipse quadrant rather than an arc. Radii that would
/// overlap are shrunk together rather than clamped one at a time.
pub fn radii(
    style: &ComputedStyle,
    border_box: Rect<DevicePx, Device>,
    scale: f32,
) -> Corners<Vec2<DevicePx>> {
    let border = style.get_border();
    let width = border_box.size.width.0;
    let height = border_box.size.height.0;
    let corner = |value: &BorderCornerRadiusValue| {
        Vec2::new(
            DevicePx(component(&value.0.width.0, width, scale)),
            DevicePx(component(&value.0.height.0, height, scale)),
        )
    };
    Corners {
        top_left: corner(&border.border_top_left_radius),
        top_right: corner(&border.border_top_right_radius),
        bottom_right: corner(&border.border_bottom_right_radius),
        bottom_left: corner(&border.border_bottom_left_radius),
    }
    .fit_within(border_box.size)
}

/// The radii of the padding box, which are the border box's radii less the border widths.
///
/// A curve inside a border is concentric with it rather than parallel: subtracting the border width
/// from the radius is what keeps the inner and outer curves the same distance apart all the way
/// round. A radius smaller than the border it sits inside collapses to a square corner.
fn inner_radii(
    style: &ComputedStyle,
    padding_box: Rect<DevicePx, Device>,
    border: Edges<DevicePx>,
    scale: f32,
) -> Corners<Vec2<DevicePx>> {
    let outer = radii(style, padding_box.outset(border), scale);
    let inset = |radius: Vec2<DevicePx>, horizontal: DevicePx, vertical: DevicePx| {
        Vec2::new(
            DevicePx((radius.x.0 - horizontal.0).max(0.0)),
            DevicePx((radius.y.0 - vertical.0).max(0.0)),
        )
    };
    Corners {
        top_left: inset(outer.top_left, border.left, border.top),
        top_right: inset(outer.top_right, border.right, border.top),
        bottom_right: inset(outer.bottom_right, border.right, border.bottom),
        bottom_left: inset(outer.bottom_left, border.left, border.bottom),
    }
    .fit_within(padding_box.size)
}

/// One radius component, with a percentage taken of the extent it is measured along.
fn component(value: &LengthPercentage, basis: f32, scale: f32) -> f32 {
    evaluate_at(value, CssPx(basis / scale)).0 * scale
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;
    use zgui_geom::{Device, DevicePx, Edges, Point, Rect, Size};
    use zgui_scene::{ClipId, ClipTable};

    use super::{chain_for_children, clips_children, inner_radii};

    /// A box nothing has carried anywhere.
    const UNMOVED: Size<DevicePx, Device> = Size::ZERO;

    /// A 100 by 50 box at the origin.
    fn rect() -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(100.0), DevicePx(50.0)),
        )
    }

    /// An owner to name chains after.
    fn owner(word: u64) -> zgui_scene::PropertyOwner {
        zgui_scene::PropertyOwner::new(word).expect("a non-zero word names an owner")
    }

    #[test]
    fn a_box_that_clips_nothing_leaves_the_chain_it_was_given() {
        let style = StyleDraft::initial().build();
        assert!(!clips_children(&style));
        let mut clips = ClipTable::rooted();
        let chain = chain_for_children(
            &mut clips,
            ClipId::ROOT,
            &style,
            rect(),
            Edges::ZERO,
            1.0,
            UNMOVED,
            zgui_scene::SpatialId::VIEWPORT,
            owner(1),
        );
        assert_eq!(chain, ClipId::ROOT);
        assert_eq!(clips.len(), 1, "no link was interned");
    }

    #[test]
    fn a_clipping_box_carried_by_a_scroll_keeps_the_chain_it_had() {
        let mut draft = StyleDraft::initial();
        draft.box_group().overflow_y = zgui_css::values::size::OverflowValue::Hidden;
        let style = draft.build();
        assert!(clips_children(&style));
        let mut clips = ClipTable::rooted();
        let first = chain_for_children(
            &mut clips,
            ClipId::ROOT,
            &style,
            rect(),
            Edges::ZERO,
            1.0,
            UNMOVED,
            zgui_scene::SpatialId::VIEWPORT,
            owner(1),
        );
        assert_eq!(clips.len(), 2, "the root and the box's own link");

        // The same box, one notch further along: drawn eleven pixels higher, and carried there by
        // the scroll above it.
        let by = Size::new(DevicePx(0.0), DevicePx(-11.0));
        let again = chain_for_children(
            &mut clips,
            ClipId::ROOT,
            &style,
            rect().translate(by),
            Edges::ZERO,
            1.0,
            by,
            zgui_scene::SpatialId::VIEWPORT,
            owner(1),
        );
        assert_eq!(again, first, "the same clipping box is the same chain");
        assert_eq!(clips.len(), 2, "and nothing was interned for the movement");
        assert_eq!(
            clips.bounds(again),
            rect().translate(by),
            "while the chain clips where the box now is"
        );
    }

    #[test]
    fn a_clipping_box_laid_out_to_a_new_extent_keeps_the_chain_it_had() {
        let mut draft = StyleDraft::initial();
        draft.box_group().overflow_y = zgui_css::values::size::OverflowValue::Hidden;
        let style = draft.build();
        let mut clips = ClipTable::rooted();
        let first = chain_for_children(
            &mut clips,
            ClipId::ROOT,
            &style,
            rect(),
            Edges::ZERO,
            1.0,
            UNMOVED,
            zgui_scene::SpatialId::VIEWPORT,
            owner(1),
        );

        // The same box after a resize: the window is wider, so layout gave it a wider padding box.
        let wider = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(148.0), DevicePx(50.0)),
        );
        let again = chain_for_children(
            &mut clips,
            ClipId::ROOT,
            &style,
            wider,
            Edges::ZERO,
            1.0,
            UNMOVED,
            zgui_scene::SpatialId::VIEWPORT,
            owner(1),
        );
        assert_eq!(again, first, "the same clipping box is the same chain");
        assert_eq!(clips.len(), 2, "and nothing was interned for the resize");
        assert_eq!(
            clips.bounds(again),
            wider,
            "while the chain clips the extent the box now has"
        );
    }

    #[test]
    fn an_inner_radius_is_the_outer_one_less_the_border() {
        let style = StyleDraft::initial().build();
        // The initial style has no radius at all, so the inner radii are square whatever the
        // border is — which is the case that must not accidentally produce a negative radius.
        let border = Edges {
            top: DevicePx(4.0),
            right: DevicePx(4.0),
            bottom: DevicePx(4.0),
            left: DevicePx(4.0),
        };
        let radii = inner_radii(&style, rect(), border, 1.0);
        assert!(radii.is_square());
    }
}
