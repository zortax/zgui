//! The damage a window's frame declares to its compositor.

use compositor_types::compositor_damage::DamageSet as Declared;
use compositor_types::compositor_geom::{Point, Rect, Size};
use zgui_bits::DamageSet;

/// Returns `painted`, the damage the frame's paint walk drew, as the compositor's damage set.
pub(super) fn declared(painted: &DamageSet) -> Declared {
    if painted.is_full() {
        return Declared::full();
    }
    let mut declared = Declared::new();
    for rect in painted.rects() {
        declared.absorb(Rect::new(
            Point::new(rect.origin.x, rect.origin.y),
            Size::new(rect.size.width, rect.size.height),
        ));
    }
    declared
}
