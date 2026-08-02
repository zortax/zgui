//! Whether one composite can carry a whole pass without covering something that belongs above it.
//!
//! A pass is composited by **one** draw, and that draw has to sit above every item in the pass or
//! the enclosing backgrounds of the deepest item erase it. Sitting above every item is not free:
//! anything painted over one of the pass's *lower* items belongs above that item but below the
//! composite, and one draw cannot be in two places.
//!
//! Rule 3 already keeps that from happening while a pass is accumulating, but it is only consulted
//! when the next item arrives. A primitive painted after a pass's final item is recorded and never
//! tested, and so is one that arrives when no further item follows it. This is the test for what
//! those leave behind, applied once the pass is complete and every item's order is known.

use zgui_geom::{Device, DevicePx, Rect};

use crate::id::DrawOrder;
use crate::pass::overlap::Intervening;

/// Whether compositing these items as one draw at `composite` would cover a primitive that belongs
/// above one of them.
///
/// `inks` and `orders` describe the pass's items in the order they were admitted, and `intervening`
/// the non-vector primitives emitted alongside them. A primitive emitted *after* an item and
/// overlapping that item's ink was given an order above it by the bounds tree, so it belongs above
/// that item; if the item is not the highest in the pass, the one composite is drawn above the
/// primitive and the item shows through it.
pub(crate) fn traps(
    inks: &[Rect<DevicePx, Device>],
    orders: &[DrawOrder],
    intervening: &[Intervening],
    composite: DrawOrder,
) -> bool {
    intervening.iter().any(|primitive| {
        let earlier = primitive.accumulated.min(inks.len());
        (0..earlier)
            .any(|index| orders[index] < composite && primitive.bounds.intersects(inks[index]))
    })
}
