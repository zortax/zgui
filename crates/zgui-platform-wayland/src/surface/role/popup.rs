//! A pop-up: where the compositor is asked to put it, and what it may do to fit it on screen.

use wayland_protocols::xdg::shell::client::xdg_positioner::{
    Anchor, ConstraintAdjustment, Gravity, XdgPositioner,
};
use zgui_geom::{Css, CssPx, Rect};
use zgui_platform::{Constrain, PopupPlacement};

/// Which point of the anchor rectangle the pop-up hangs from.
pub const fn anchor(anchor: zgui_platform::Anchor) -> Anchor {
    match anchor {
        zgui_platform::Anchor::Top => Anchor::Top,
        zgui_platform::Anchor::Bottom => Anchor::Bottom,
        zgui_platform::Anchor::Left => Anchor::Left,
        zgui_platform::Anchor::Right => Anchor::Right,
        zgui_platform::Anchor::TopLeft => Anchor::TopLeft,
        zgui_platform::Anchor::TopRight => Anchor::TopRight,
        zgui_platform::Anchor::BottomLeft => Anchor::BottomLeft,
        zgui_platform::Anchor::BottomRight => Anchor::BottomRight,
        _ => Anchor::None,
    }
}

/// Which way the pop-up extends from the anchor point.
pub const fn gravity(anchor: zgui_platform::Anchor) -> Gravity {
    match anchor {
        zgui_platform::Anchor::Top => Gravity::Top,
        zgui_platform::Anchor::Bottom => Gravity::Bottom,
        zgui_platform::Anchor::Left => Gravity::Left,
        zgui_platform::Anchor::Right => Gravity::Right,
        zgui_platform::Anchor::TopLeft => Gravity::TopLeft,
        zgui_platform::Anchor::TopRight => Gravity::TopRight,
        zgui_platform::Anchor::BottomLeft => Gravity::BottomLeft,
        zgui_platform::Anchor::BottomRight => Gravity::BottomRight,
        _ => Gravity::None,
    }
}

/// What the compositor may do to a pop-up that would not fit.
pub fn constraint(constrain: Constrain) -> ConstraintAdjustment {
    let mut adjustment = ConstraintAdjustment::None;
    if constrain.slide {
        adjustment |= ConstraintAdjustment::SlideX | ConstraintAdjustment::SlideY;
    }
    if constrain.flip {
        adjustment |= ConstraintAdjustment::FlipX | ConstraintAdjustment::FlipY;
    }
    if constrain.resize {
        adjustment |= ConstraintAdjustment::ResizeX | ConstraintAdjustment::ResizeY;
    }
    adjustment
}

/// The anchor rectangle, clamped into the parent's geometry and to a non-empty extent.
///
/// Both are protocol errors rather than mistakes the compositor forgives: a rectangle of zero
/// extent, and one that leaves the parent's window geometry, each disconnect the client. The
/// rectangle a menu is anchored to is computed from a layout that knows nothing about either, so
/// it is corrected here — pulled inward at the edges rather than refused, because a menu placed a
/// pixel from where it was asked for is a better answer than no menu.
pub fn anchor_rect(
    rect: Rect<CssPx, Css>,
    parent: zgui_geom::Size<CssPx, Css>,
) -> (i32, i32, i32, i32) {
    let limit_x = parent.width.0.max(1.0).round() as i32;
    let limit_y = parent.height.0.max(1.0).round() as i32;
    let x = (rect.origin.x.0.round() as i32).clamp(0, limit_x - 1);
    let y = (rect.origin.y.0.round() as i32).clamp(0, limit_y - 1);
    let width = (rect.size.width.0.round() as i32).clamp(1, limit_x - x);
    let height = (rect.size.height.0.round() as i32).clamp(1, limit_y - y);
    (x, y, width, height)
}

/// Fills `positioner` in from `placement`, measured against a parent of `parent` logical pixels.
pub fn describe(
    positioner: &XdgPositioner,
    placement: &PopupPlacement,
    parent: zgui_geom::Size<CssPx, Css>,
    size: zgui_geom::Size<CssPx, Css>,
) {
    let (x, y, width, height) = anchor_rect(placement.anchor_rect, parent);
    positioner.set_anchor_rect(x, y, width, height);
    positioner.set_anchor(anchor(placement.anchor));
    positioner.set_gravity(gravity(placement.gravity));
    positioner.set_constraint_adjustment(constraint(placement.constrain));
    positioner.set_size(
        (size.width.0.round() as i32).max(1),
        (size.height.0.round() as i32).max(1),
    );
}

#[cfg(test)]
mod tests {
    use super::{anchor, anchor_rect, constraint, gravity};
    use wayland_protocols::xdg::shell::client::xdg_positioner::{
        Anchor, ConstraintAdjustment, Gravity,
    };
    use zgui_geom::{CssPx, Point, Rect, Size};
    use zgui_platform::Constrain;

    fn parent() -> Size<CssPx, zgui_geom::Css> {
        Size::new(CssPx(400.0), CssPx(300.0))
    }

    #[test]
    fn a_drop_down_hangs_from_the_bottom_left_and_extends_down_and_right() {
        assert_eq!(
            anchor(zgui_platform::Anchor::BottomLeft),
            Anchor::BottomLeft
        );
        assert_eq!(
            gravity(zgui_platform::Anchor::BottomRight),
            Gravity::BottomRight
        );
        assert_eq!(anchor(zgui_platform::Anchor::Center), Anchor::None);
    }

    #[test]
    fn each_freedom_the_compositor_is_given_covers_both_axes() {
        assert_eq!(constraint(Constrain::NONE), ConstraintAdjustment::None);
        assert_eq!(
            constraint(Constrain::SLIDE),
            ConstraintAdjustment::SlideX | ConstraintAdjustment::SlideY
        );
        assert!(constraint(Constrain::ANY).contains(ConstraintAdjustment::FlipY));
        assert!(constraint(Constrain::ANY).contains(ConstraintAdjustment::ResizeX));
    }

    #[test]
    fn a_rectangle_inside_the_parent_crosses_unchanged() {
        let rect = Rect::new(
            Point::new(CssPx(10.0), CssPx(20.0)),
            Size::new(CssPx(100.0), CssPx(24.0)),
        );
        assert_eq!(anchor_rect(rect, parent()), (10, 20, 100, 24));
    }

    #[test]
    fn a_rectangle_of_no_extent_is_grown_rather_than_sent_as_a_protocol_error() {
        // A zero-sized anchor rectangle disconnects the client. A caret is exactly that.
        let caret = Rect::new(
            Point::new(CssPx(40.0), CssPx(40.0)),
            Size::new(CssPx(0.0), CssPx(0.0)),
        );
        assert_eq!(anchor_rect(caret, parent()), (40, 40, 1, 1));
    }

    #[test]
    fn a_rectangle_hanging_off_the_parent_is_pulled_back_inside_it() {
        let hanging = Rect::new(
            Point::new(CssPx(380.0), CssPx(290.0)),
            Size::new(CssPx(200.0), CssPx(200.0)),
        );
        let (x, y, width, height) = anchor_rect(hanging, parent());
        assert_eq!((x, y), (380, 290));
        assert_eq!((width, height), (20, 10));
        assert!(x + width <= 400 && y + height <= 300);
    }

    #[test]
    fn a_rectangle_starting_before_the_parent_is_moved_to_its_origin() {
        let before = Rect::new(
            Point::new(CssPx(-30.0), CssPx(-30.0)),
            Size::new(CssPx(10.0), CssPx(10.0)),
        );
        assert_eq!(anchor_rect(before, parent()), (0, 0, 10, 10));
    }
}
