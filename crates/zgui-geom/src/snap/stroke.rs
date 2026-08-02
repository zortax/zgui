// DERIVED-FROM: the GPUI project, crates/gpui/src/window.rs and crates/gpui/src/util.rs (Apache-2.0)
// The stroke snapping rule — round to the nearest device pixel, but never let a stroke that was
// asked for round away to nothing — is adapted from that work, which is licensed under the
// Apache License, Version 2.0. It has been modified to work over this crate's spaces and units.

//! Stroke width snapping.

use crate::edges::Edges;
use crate::snap::snap_length;
use crate::space::{Css, Device};
use crate::unit::{CssPx, DevicePx, Scale, Unit};

/// Scales a stroke width into device space and rounds it onto the pixel grid, never to zero.
///
/// A width is not a position, so the rule differs from [`snap_bounds`](crate::snap_bounds) in two
/// ways. Negative widths are meaningless and clamp to zero. And a width that was asked for never
/// rounds *away*: `border: 0.4px` on a 1x display would otherwise vanish, and a border that is
/// silently absent is a much louder bug than one that is a third of a pixel too thick.
///
/// ```
/// use zgui_geom::{Css, CssPx, Device, DevicePx, Scale, snap_stroke};
///
/// let scale: Scale<Css, Device> = Scale::new(1.0);
/// assert_eq!(snap_stroke(CssPx(0.0), scale), DevicePx(0.0));
/// assert_eq!(snap_stroke(CssPx(0.4), scale), DevicePx(1.0));
/// assert_eq!(snap_stroke(CssPx(2.6), scale), DevicePx(3.0));
/// ```
pub fn snap_stroke(width: CssPx, scale: Scale<Css, Device>) -> DevicePx {
    snap_device_stroke(scale.apply_length(width))
}

/// Rounds a device-space stroke width onto the pixel grid, never to zero.
///
/// [`snap_stroke`] is this rule preceded by the scale. Applying it twice changes nothing.
pub fn snap_device_stroke(width: DevicePx) -> DevicePx {
    if width.0 <= 0.0 {
        return DevicePx(0.0);
    }
    Unit::max(snap_length(width), DevicePx(1.0))
}

/// Snaps all four border widths of a box, each by the rule in [`snap_stroke`].
///
/// ```
/// use zgui_geom::{Css, CssPx, Device, DevicePx, Edges, Scale, snap_edges};
///
/// let scale: Scale<Css, Device> = Scale::new(2.0);
/// let border = Edges::new(CssPx(1.0), CssPx(0.0), CssPx(0.2), CssPx(1.0));
/// assert_eq!(
///     snap_edges(border, scale),
///     Edges::new(DevicePx(2.0), DevicePx(0.0), DevicePx(1.0), DevicePx(2.0)),
/// );
/// ```
pub fn snap_edges(edges: Edges<CssPx>, scale: Scale<Css, Device>) -> Edges<DevicePx> {
    edges.map(|width| snap_stroke(width, scale))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::{snap_device_stroke, snap_stroke};
    use crate::space::{Css, Device};
    use crate::unit::{CssPx, DevicePx, Scale};

    #[test]
    fn a_zero_width_stays_zero_and_a_negative_one_clamps() {
        let scale: Scale<Css, Device> = Scale::new(3.0);
        assert_eq!(snap_stroke(CssPx(0.0), scale), DevicePx(0.0));
        assert_eq!(snap_stroke(CssPx(-4.0), scale), DevicePx(0.0));
    }

    proptest! {
        /// A requested stroke never rounds away, and an unrequested one never appears.
        #[test]
        fn a_requested_stroke_survives(width in 0.0_f32..64.0, factor in 0.25_f32..4.0) {
            let scale: Scale<Css, Device> = Scale::new(factor);
            let snapped = snap_stroke(CssPx(width), scale);
            if width > 0.0 {
                prop_assert!(snapped >= DevicePx(1.0));
            } else {
                prop_assert_eq!(snapped, DevicePx(0.0));
            }
        }

        /// Snapping an already-snapped width changes nothing.
        #[test]
        fn stroke_snapping_is_idempotent(width in 0.0_f32..64.0, factor in 0.25_f32..4.0) {
            let scale: Scale<Css, Device> = Scale::new(factor);
            let once = snap_stroke(CssPx(width), scale);
            prop_assert_eq!(snap_device_stroke(once), once);
        }

        /// A snapped width lands on the grid.
        #[test]
        fn snapped_widths_land_on_the_grid(width in 0.0_f32..64.0, factor in 0.25_f32..4.0) {
            let scale: Scale<Css, Device> = Scale::new(factor);
            prop_assert!(snap_stroke(CssPx(width), scale).is_grid_aligned());
        }
    }
}
