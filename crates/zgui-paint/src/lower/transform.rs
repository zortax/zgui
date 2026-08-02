//! The transform a box is drawn under, and the three properties that force it into a target.
//!
//! The matrix itself is layout's: a transform changes where a box's descendants are, so the
//! fragment tree has already composed it and named the coordinate system it belongs to, and a
//! fragment carries that name. Re-deriving it here would be a second opinion about where a box is.
//!
//! What is left is the compositing question. A two-dimensional transform is applied by the draw
//! call and needs nothing; a *three-dimensional* one does not compose that way — perspective, a
//! preserved 3D context and a hidden back face all need the subtree drawn once, into a target,
//! before it is placed.

use zgui_css::parity::Support;
use zgui_css::{ComputedStyle, register_properties};
use zgui_layout::Fragment;
use zgui_scene::SpatialId;

register_properties! {
    transform            => Support::Implemented("zgui-layout::fragment::transform"),
    translate            => Support::Implemented("zgui-layout::fragment::transform"),
    rotate               => Support::Implemented("zgui-layout::fragment::transform"),
    scale                => Support::Implemented("zgui-layout::fragment::transform"),
    transform_origin     => Support::Implemented("zgui-layout::fragment::transform"),
    perspective          => Support::Implemented("zgui-paint::lower::transform"),
    perspective_origin   => Support::Ignored("the perspective matrix is not composed yet"),
    transform_style      => Support::Implemented("zgui-paint::lower::transform"),
    backface_visibility  => Support::Implemented("zgui-paint::lower::transform"),
}

/// The coordinate system a fragment's primitives are drawn in.
///
/// Every primitive carries one, so the viewport is a real answer rather than an absence — and it is
/// the name the layout stage established, not a fresh one.
pub fn of(fragment: &Fragment) -> SpatialId {
    fragment.transform.unwrap_or(SpatialId::VIEWPORT)
}

/// Whether the box's transform properties force it to composite into a target of its own.
///
/// Perspective and a preserved three-dimensional context both mean descendants are placed in a
/// space this stage does not flatten, and a hidden back face is a test that can only be made once
/// the subtree exists as an image. A plain two-dimensional `transform` is none of those, which is
/// the overwhelming majority of transformed boxes.
pub fn forces_group(style: &ComputedStyle) -> bool {
    use zgui_css::values::transform::{
        BackfaceVisibilityValue, PerspectiveValue, TransformStyleValue,
    };
    let box_ = style.get_box();
    !matches!(box_.perspective, PerspectiveValue::None)
        || box_.transform_style == TransformStyleValue::Preserve3d
        || box_.backface_visibility == BackfaceVisibilityValue::Hidden
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;

    use super::forces_group;

    #[test]
    fn a_plain_box_composites_in_place() {
        assert!(!forces_group(&StyleDraft::initial().build()));
    }
}
