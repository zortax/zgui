//! The damage a window's frame declares to its compositor.

use compositor_types::DeviceRect;
use compositor_types::compositor_damage::DamageSet as Declared;
use compositor_types::compositor_geom::{Point, Rect, Size};
use zgui_bits::DamageSet;
use zgui_geom::Device;

/// Returns the damage the frame's paint walk drew, `painted`, and the ink of the fragments it
/// recorded, `recorded`, as the compositor's damage set.
pub(super) fn declared(painted: &DamageSet, recorded: &Declared) -> Declared {
    if painted.is_full() {
        return Declared::full();
    }
    let mut declared = *recorded;
    for rect in painted.rects() {
        declared.absorb(of(*rect));
    }
    declared
}

/// Returns the pixels the frame's paint walk drew, `painted`, and the ink of the fragments it
/// recorded, `recorded`, as one set that reaches past the surface.
pub(super) fn visited(painted: &DamageSet, recorded: &Declared) -> DamageSet {
    let mut visited = DamageSet::new();
    if painted.is_full() {
        visited.set_full();
        return visited;
    }
    for rect in painted.rects() {
        visited.absorb(*rect);
    }
    for rect in recorded.rects() {
        visited.absorb(zgui_geom::Rect::new(
            zgui_geom::Point::new(rect.origin.x, rect.origin.y),
            zgui_geom::Size::new(rect.size.width, rect.size.height),
        ));
    }
    visited
}

/// Returns `rect` as the compositor's rectangle.
pub(super) fn of(rect: zgui_geom::Rect<i32, Device>) -> DeviceRect {
    Rect::new(
        Point::new(rect.origin.x, rect.origin.y),
        Size::new(rect.size.width, rect.size.height),
    )
}
