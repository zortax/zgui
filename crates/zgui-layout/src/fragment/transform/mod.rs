//! The matrix a box's transform properties compose to.
//!
//! Four properties contribute — `translate`, `rotate`, `scale` and `transform` — and CSS applies
//! them in exactly that order, each about the same origin. The origin matters: a rotation about a
//! box's centre and a rotation about its top-left corner are different transforms, so the matrix
//! handed downstream is *translate to the origin, transform, translate back*, resolved against the
//! box's own border box.

pub mod animated;
pub mod ops;
pub mod placed;

use zgui_css::computed::style::style_structs;
use zgui_css::values::length::evaluate_at;
use zgui_geom::{Device, DevicePx, Matrix4, Rect};

use crate::fragment::transform::ops::operation;

pub use zgui_geom::transformed_bounds;

/// Whether any of the four transform properties moves this box.
///
/// Asked on its own as well as through [`matrix_of`], because whether a box is transformed decides
/// whether it establishes a stacking context — a question with no geometry in it at all.
pub fn is_transformed(box_: &style_structs::Box) -> bool {
    !box_.transform.0.is_empty()
        || !matches!(
            box_.translate,
            zgui_css::values::transform::TranslateValue::None
        )
        || !matches!(box_.rotate, zgui_css::values::transform::RotateValue::None)
        || !matches!(box_.scale, zgui_css::values::transform::ScaleValue::None)
}

/// The matrix `border_box` is drawn under, or nothing when the box is not transformed.
///
/// The rectangle is in device pixels and absolute, and the matrix maps that space to itself, so a
/// caller composes it with an ancestor's matrix by ordinary multiplication and never has to know
/// where either box sits.
pub fn matrix_of(
    box_: &style_structs::Box,
    border_box: Rect<DevicePx, Device>,
    scale: f32,
) -> Option<Matrix4> {
    if !is_transformed(box_) {
        return None;
    }

    let width = border_box.size.width.0;
    let height = border_box.size.height.0;
    // Application order, not written order. A `transform` list applies right to left — the last
    // function written is the first a point passes through — and the three individual properties
    // apply after the whole list, scale first and translate last.
    let mut matrix = Matrix4::IDENTITY;
    for operation_value in box_.transform.0.iter().rev() {
        matrix = matrix.then(&operation(operation_value, width, height, scale));
    }
    for step in ops::individual(box_, width, height, scale) {
        matrix = matrix.then(&step);
    }

    let (origin_x, origin_y, origin_z) = origin(box_, border_box, scale);
    let to_origin = Matrix4::translation(origin_x, origin_y, origin_z);
    let back = Matrix4::translation(-origin_x, -origin_y, -origin_z);
    Some(back.then(&matrix).then(&to_origin))
}

/// Where a box's transform origin sits, in the same absolute space as its border box.
fn origin(
    box_: &style_structs::Box,
    border_box: Rect<DevicePx, Device>,
    scale: f32,
) -> (f32, f32, f32) {
    let origin = &box_.transform_origin;
    let x = evaluate_at(
        &origin.horizontal,
        zgui_geom::CssPx(border_box.size.width.0 / scale),
    )
    .0 * scale;
    let y = evaluate_at(
        &origin.vertical,
        zgui_geom::CssPx(border_box.size.height.0 / scale),
    )
    .0 * scale;
    (
        border_box.origin.x.0 + x,
        border_box.origin.y.0 + y,
        origin.depth.px() * scale,
    )
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;
    use zgui_geom::{Device, DevicePx, Matrix4, Point, Rect, Size};

    use super::{matrix_of, transformed_bounds};

    /// A 100 by 40 box at (10, 20).
    fn box_rect() -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(10.0), DevicePx(20.0)),
            Size::new(DevicePx(100.0), DevicePx(40.0)),
        )
    }

    #[test]
    fn an_untransformed_box_has_no_matrix_at_all() {
        let style = StyleDraft::initial().build();
        assert!(matrix_of(style.get_box(), box_rect(), 1.0).is_none());
    }

    #[test]
    fn bounds_under_the_identity_are_the_rectangle_itself() {
        let rect = box_rect();
        assert_eq!(transformed_bounds(&Matrix4::IDENTITY, rect), rect);
    }

    #[test]
    fn bounds_under_a_translation_move_by_it() {
        let rect = box_rect();
        let moved = transformed_bounds(&Matrix4::translation(5.0, -7.0, 0.0), rect);
        assert_eq!(moved.origin, Point::new(DevicePx(15.0), DevicePx(13.0)));
        assert_eq!(moved.size, rect.size);
    }
}
