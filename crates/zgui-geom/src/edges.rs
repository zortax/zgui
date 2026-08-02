//! Per-side lengths around a box.

use core::ops::{Add, AddAssign, Sub, SubAssign};

use crate::size::Size;
use crate::space::Space;
use crate::unit::Unit;

/// Four lengths, one per side of a box: a border, a padding, a margin or an inset.
///
/// There is no space parameter, because an edge width is a length rather than a position: the
/// same `Edges<CssPx>` describes the border of any box in any space. It gains a space the moment
/// it is applied to something that has one, through
/// [`Rect::inset`](crate::Rect::inset) or [`Rect::outset`](crate::Rect::outset).
///
/// ```
/// use zgui_geom::{Css, CssPx, Edges, Point, Rect, Size};
///
/// let border = Edges::uniform(CssPx(2.0));
/// let border_box: Rect<CssPx, Css> = Rect::new(
///     Point::new(CssPx(0.0), CssPx(0.0)),
///     Size::new(CssPx(100.0), CssPx(50.0)),
/// );
/// let padding_box = border_box.inset(border);
/// assert_eq!(padding_box.size, Size::new(CssPx(96.0), CssPx(46.0)));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Edges<T> {
    /// The length along the top side.
    pub top: T,
    /// The length along the right side.
    pub right: T,
    /// The length along the bottom side.
    pub bottom: T,
    /// The length along the left side.
    pub left: T,
}

impl<T> Edges<T> {
    /// Edge lengths given in the CSS shorthand order: top, right, bottom, left.
    pub const fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Applies a function to every side, possibly changing the unit.
    pub fn map<U>(self, mut function: impl FnMut(T) -> U) -> Edges<U> {
        Edges {
            top: function(self.top),
            right: function(self.right),
            bottom: function(self.bottom),
            left: function(self.left),
        }
    }

    /// The four lengths in the CSS shorthand order: top, right, bottom, left.
    pub fn into_array(self) -> [T; 4] {
        [self.top, self.right, self.bottom, self.left]
    }
}

impl<T: Copy> Edges<T> {
    /// The same length on all four sides.
    pub const fn uniform(length: T) -> Self {
        Self {
            top: length,
            right: length,
            bottom: length,
            left: length,
        }
    }

    /// The given lengths on the horizontal and vertical pairs of sides.
    pub const fn axes(horizontal: T, vertical: T) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
}

impl<T: Unit> Edges<T> {
    /// No length on any side.
    pub const ZERO: Self = Self {
        top: T::ZERO,
        right: T::ZERO,
        bottom: T::ZERO,
        left: T::ZERO,
    };

    /// The total length taken from the horizontal axis, that is left plus right.
    pub fn horizontal(self) -> T {
        self.left + self.right
    }

    /// The total length taken from the vertical axis, that is top plus bottom.
    pub fn vertical(self) -> T {
        self.top + self.bottom
    }

    /// The extent these edges consume, as a size in space `S`.
    ///
    /// ```
    /// use zgui_geom::{Css, CssPx, Edges, Size};
    ///
    /// let padding = Edges::uniform(CssPx(4.0));
    /// assert_eq!(padding.total::<Css>(), Size::new(CssPx(8.0), CssPx(8.0)));
    /// ```
    pub fn total<S: Space>(self) -> Size<T, S> {
        Size::new(self.horizontal(), self.vertical())
    }

    /// Whether every side is zero.
    pub fn is_zero(self) -> bool {
        self == Self::ZERO
    }

    /// The edges with every side clamped to at least zero.
    pub fn non_negative(self) -> Self {
        self.map(|length| length.max(T::ZERO))
    }

    /// The edges with every side converted to another unit through [`f32`].
    pub fn to_unit<U: Unit>(self) -> Edges<U> {
        self.map(|length| U::from_f32(length.to_f32()))
    }
}

impl<T: Unit> Add for Edges<T> {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self::new(
            self.top + other.top,
            self.right + other.right,
            self.bottom + other.bottom,
            self.left + other.left,
        )
    }
}

impl<T: Unit> AddAssign for Edges<T> {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl<T: Unit> Sub for Edges<T> {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self::new(
            self.top - other.top,
            self.right - other.right,
            self.bottom - other.bottom,
            self.left - other.left,
        )
    }
}

impl<T: Unit> SubAssign for Edges<T> {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

#[cfg(test)]
mod tests {
    use super::Edges;
    use crate::size::Size;
    use crate::space::Css;
    use crate::unit::CssPx;

    #[test]
    fn the_shorthand_order_is_top_right_bottom_left() {
        let edges = Edges::new(CssPx(1.0), CssPx(2.0), CssPx(3.0), CssPx(4.0));
        assert_eq!(
            edges.into_array(),
            [CssPx(1.0), CssPx(2.0), CssPx(3.0), CssPx(4.0)]
        );
        assert_eq!(edges.horizontal(), CssPx(6.0));
        assert_eq!(edges.vertical(), CssPx(4.0));
    }

    #[test]
    fn axes_puts_the_horizontal_length_on_left_and_right() {
        let edges = Edges::axes(CssPx(10.0), CssPx(2.0));
        assert_eq!(edges.left, CssPx(10.0));
        assert_eq!(edges.right, CssPx(10.0));
        assert_eq!(edges.top, CssPx(2.0));
        assert_eq!(edges.bottom, CssPx(2.0));
    }

    #[test]
    fn the_total_is_a_size_in_the_requested_space() {
        let edges = Edges::uniform(CssPx(3.0));
        let total: Size<CssPx, Css> = edges.total();
        assert_eq!(total, Size::new(CssPx(6.0), CssPx(6.0)));
    }
}
