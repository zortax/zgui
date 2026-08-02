//! The properties the engine calls layout-affecting that this pipeline only draws.
//!
//! The engine's relayout predicate names border colours, corner radii, box shadows, clips and masks
//! because the layout it was written for builds painting fragments inside its boxes. Here they are
//! read while a fragment is measured and while it is drawn, and never while anything is sized or
//! placed.

use style::properties::ComputedValues;

use crate::stylo::element::damage::classify::Cost;
use crate::stylo::element::damage::classify::same::same;

/// What the difference between two styles costs, given that nothing laid out from has moved.
///
/// Finding no difference at all is not "nothing changed": the engine calls the hook this feeds only
/// after deciding something layout-affecting did, so it means the property responsible is one this
/// classification does not name, and the widest answer is the only safe reading of that.
pub(super) fn cost(old: &ComputedValues, new: &ComputedValues) -> Cost {
    if !covers(old, new) {
        return Cost::Ink;
    }
    if !colours(old, new) {
        return Cost::Repaint;
    }
    Cost::Layout
}

/// Whether the shape of the area the element covers agrees.
///
/// A corner radius clips what is inside it, a shadow and a filter reach outside the border box, and
/// a mask decides which of the covered pixels survive. All of them change the fragment's ink and
/// the region a hit test has to consider, and none of them changes a size or a position.
fn covers(old: &ComputedValues, new: &ComputedValues) -> bool {
    same!(
        old,
        new,
        get_border,
        [
            border_top_left_radius,
            border_top_right_radius,
            border_bottom_right_radius,
            border_bottom_left_radius,
            corner_top_left_shape,
            corner_top_right_shape,
            corner_bottom_right_shape,
            corner_bottom_left_shape,
        ]
    ) && same!(old, new, get_effects, [backdrop_filter, box_shadow, clip])
        && same!(
            old,
            new,
            get_svg,
            [
                mask_clip,
                mask_composite,
                mask_image,
                mask_mode,
                mask_origin,
                mask_position_x,
                mask_position_y,
                mask_repeat,
                mask_size,
                mask_type,
            ]
        )
        && same!(old, new, get_inherited_box, [image_rendering])
        && same!(old, new, get_inherited_ui, [color_scheme])
}

/// Whether the colours drawn into that area agree.
fn colours(old: &ComputedValues, new: &ComputedValues) -> bool {
    same!(
        old,
        new,
        get_border,
        [
            border_top_color,
            border_right_color,
            border_bottom_color,
            border_left_color,
        ]
    )
}
