//! The engine's own length unit and geometry, and the conversions to this framework's.

use zgui_geom::CssPx;

/// The engine's exact length: one sixtieth of a CSS pixel, as an integer.
///
/// Lengths accumulate — a border plus a padding plus a content width, a hundred times down a
/// column — and a float accumulates error while an integer does not, which is why the engine
/// measures in these rather than in pixels.
pub use app_units::Au;
/// A scale factor between two coordinate spaces, which is how a device pixel ratio is stated.
pub use euclid::Scale;
/// A width and a height, tagged with the space they were measured in.
pub use euclid::Size2D;

/// The size of the nearest query container, in the engine's own unit.
///
/// Either dimension is absent when no container establishes it, which is not the same as zero: a
/// query against a dimension nothing constrains cannot be answered, and must not be answered with
/// a number.
pub type ContainerSize = euclid::default::Size2D<Option<Au>>;

/// The engine's length in this framework's unit.
///
/// Exact in both directions for any value either can hold, because both count sixtieths of a CSS
/// pixel:
///
/// ```
/// use zgui_css::engine::geometry::{Au, from_au, to_au};
///
/// let third_of_a_pixel = Au(20);
/// assert_eq!(to_au(from_au(third_of_a_pixel)), third_of_a_pixel);
/// ```
pub fn from_au(length: Au) -> zgui_geom::Au {
    zgui_geom::Au(length.0)
}

/// This framework's length in the engine's unit.
pub fn to_au(length: zgui_geom::Au) -> Au {
    Au(length.0)
}

/// A container size in this framework's own geometry, one dimension at a time.
///
/// ```
/// use zgui_css::engine::geometry::{Au, ContainerSize, container_width};
/// use zgui_geom::CssPx;
///
/// let container = ContainerSize::new(Some(Au(60 * 320)), None);
/// assert_eq!(container_width(container), Some(CssPx(320.0)));
/// ```
pub fn container_width(size: ContainerSize) -> Option<CssPx> {
    size.width.map(|width| from_au(width).to_css_px())
}

/// The block-axis half of [`container_width`].
pub fn container_height(size: ContainerSize) -> Option<CssPx> {
    size.height.map(|height| from_au(height).to_css_px())
}

#[cfg(test)]
mod tests {
    use super::{Au, ContainerSize, container_height, container_width, from_au, to_au};
    use zgui_geom::CssPx;

    #[test]
    fn a_length_survives_a_round_trip_through_both_units() {
        for raw in [i32::MIN, -61, -1, 0, 1, 59, 60, 61, 4_194_304, i32::MAX] {
            assert_eq!(to_au(from_au(Au(raw))), Au(raw));
        }
    }

    #[test]
    fn an_unconstrained_container_dimension_stays_absent() {
        let size = ContainerSize::new(None, Some(Au(60 * 24)));
        assert_eq!(container_width(size), None);
        assert_eq!(container_height(size), Some(CssPx(24.0)));
    }
}
