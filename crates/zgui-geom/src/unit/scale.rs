//! The ratio between two coordinate spaces.

use core::fmt::{self, Debug};
use core::marker::PhantomData;

use crate::space::{Css, Device, Space};

/// The ratio that converts lengths in space `Src` into lengths in space `Dst`.
///
/// The commonest instance is a display's device pixel ratio, a `Scale<Css, Device>`: on a 2x
/// display it is `Scale::new(2.0)`, and multiplying CSS geometry by it produces device geometry.
/// Because the endpoints are part of the type, a scale cannot be applied backwards or between the
/// wrong pair of spaces — [`Scale::invert`] is the only way to turn it around.
///
/// ```
/// use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Scale};
///
/// let to_device: Scale<Css, Device> = Scale::new(2.0);
/// let css: Point<CssPx, Css> = Point::new(CssPx(4.0), CssPx(8.0));
///
/// let device: Point<DevicePx, Device> = css * to_device;
/// assert_eq!(device, Point::new(DevicePx(8.0), DevicePx(16.0)));
/// assert_eq!(device * to_device.invert(), css);
/// ```
///
/// A scale converts; it does not snap. Geometry that has to land on the physical pixel grid goes
/// through [`snap_bounds`](crate::snap_bounds) or [`cover_bounds`](crate::cover_bounds), which
/// take the scale and apply a rounding policy with it.
#[repr(transparent)]
pub struct Scale<Src, Dst> {
    /// How many `Dst` units one `Src` unit is worth.
    pub(crate) factor: f32,
    /// The two spaces this ratio relates.
    pub(crate) spaces: PhantomData<(Src, Dst)>,
}

impl<Src, Dst> Scale<Src, Dst> {
    /// A ratio of `factor` destination units per source unit.
    pub const fn new(factor: f32) -> Self {
        Self {
            factor,
            spaces: PhantomData,
        }
    }

    /// The ratio as a plain number.
    pub const fn get(self) -> f32 {
        self.factor
    }

    /// The ratio that undoes this one.
    ///
    /// ```
    /// use zgui_geom::{Css, Device, Scale};
    ///
    /// let to_device: Scale<Css, Device> = Scale::new(4.0);
    /// assert_eq!(to_device.invert().get(), 0.25);
    /// ```
    pub fn invert(self) -> Scale<Dst, Src> {
        Scale::new(self.factor.recip())
    }

    /// This ratio followed by another, giving the ratio from `Src` straight to `Next`.
    pub fn then<Next>(self, next: Scale<Dst, Next>) -> Scale<Src, Next> {
        Scale::new(self.factor * next.factor)
    }
}

impl<S> Scale<S, S> {
    /// The ratio that changes nothing.
    pub const IDENTITY: Self = Self::new(1.0);
}

impl<Src, Dst> Clone for Scale<Src, Dst> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Src, Dst> Copy for Scale<Src, Dst> {}

impl<Src: Space, Dst: Space> Debug for Scale<Src, Dst> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Scale({} {} -> {})",
            self.factor,
            Src::NAME,
            Dst::NAME
        )
    }
}

impl<Src, Dst> PartialEq for Scale<Src, Dst> {
    fn eq(&self, other: &Self) -> bool {
        self.factor == other.factor
    }
}

/// Generates the conversions for one ordered pair of spaces.
///
/// A scale relates exactly two spaces and exactly two units, so writing the pairs out is what
/// keeps `point * scale` unambiguous: there is one impl per direction and the compiler needs no
/// inference to pick it.
macro_rules! conversion {
    ($src:ty => $dst:ty, $from:ty => $to:ty) => {
        impl Scale<$src, $dst> {
            /// Converts a length into the destination space.
            pub fn apply_length(self, length: $from) -> $to {
                <$to>::from(<f32>::from(length) * self.factor)
            }

            /// Converts a position into the destination space.
            pub fn apply_point(
                self,
                point: $crate::point::Point<$from, $src>,
            ) -> $crate::point::Point<$to, $dst> {
                $crate::point::Point::new(self.apply_length(point.x), self.apply_length(point.y))
            }

            /// Converts an extent into the destination space.
            pub fn apply_size(
                self,
                size: $crate::size::Size<$from, $src>,
            ) -> $crate::size::Size<$to, $dst> {
                $crate::size::Size::new(
                    self.apply_length(size.width),
                    self.apply_length(size.height),
                )
            }

            /// Converts a rectangle into the destination space.
            pub fn apply_rect(
                self,
                rect: $crate::rect::Rect<$from, $src>,
            ) -> $crate::rect::Rect<$to, $dst> {
                $crate::rect::Rect::new(self.apply_point(rect.origin), self.apply_size(rect.size))
            }

            /// Converts per-side lengths into the destination space.
            pub fn apply_edges(
                self,
                edges: $crate::edges::Edges<$from>,
            ) -> $crate::edges::Edges<$to> {
                edges.map(|length| self.apply_length(length))
            }
        }

        impl ::core::ops::Mul<Scale<$src, $dst>> for $crate::point::Point<$from, $src> {
            type Output = $crate::point::Point<$to, $dst>;

            fn mul(self, scale: Scale<$src, $dst>) -> Self::Output {
                scale.apply_point(self)
            }
        }

        impl ::core::ops::Mul<Scale<$src, $dst>> for $crate::size::Size<$from, $src> {
            type Output = $crate::size::Size<$to, $dst>;

            fn mul(self, scale: Scale<$src, $dst>) -> Self::Output {
                scale.apply_size(self)
            }
        }

        impl ::core::ops::Mul<Scale<$src, $dst>> for $crate::rect::Rect<$from, $src> {
            type Output = $crate::rect::Rect<$to, $dst>;

            fn mul(self, scale: Scale<$src, $dst>) -> Self::Output {
                scale.apply_rect(self)
            }
        }
    };
}

conversion!(Css => Device, crate::unit::CssPx => crate::unit::DevicePx);
conversion!(Device => Css, crate::unit::DevicePx => crate::unit::CssPx);

#[cfg(test)]
mod tests {
    use super::Scale;
    use crate::point::Point;
    use crate::rect::Rect;
    use crate::size::Size;
    use crate::space::{Css, Device};
    use crate::unit::{CssPx, DevicePx};

    #[test]
    fn a_scale_and_its_inverse_cancel() {
        let scale: Scale<Css, Device> = Scale::new(2.0);
        let rect: Rect<CssPx, Css> = Rect::new(
            Point::new(CssPx(1.0), CssPx(2.0)),
            Size::new(CssPx(3.0), CssPx(4.0)),
        );
        assert_eq!((rect * scale) * scale.invert(), rect);
    }

    #[test]
    fn composing_scales_multiplies_them() {
        let there: Scale<Css, Device> = Scale::new(3.0);
        let back: Scale<Device, Css> = Scale::new(0.5);
        assert_eq!(there.then(back), Scale::<Css, Css>::new(1.5));
    }

    #[test]
    fn the_identity_leaves_lengths_alone() {
        assert_eq!(Scale::<Css, Css>::IDENTITY.get(), 1.0);
    }

    #[test]
    fn scaling_reaches_the_other_unit_and_space() {
        let scale: Scale<Css, Device> = Scale::new(1.5);
        assert_eq!(scale.apply_length(CssPx(10.0)), DevicePx(15.0));
        let size: Size<CssPx, Css> = Size::new(CssPx(2.0), CssPx(4.0));
        assert_eq!(size * scale, Size::new(DevicePx(3.0), DevicePx(6.0)));
    }

    #[test]
    fn the_debug_output_names_both_spaces() {
        let scale: Scale<Css, Device> = Scale::new(2.0);
        assert_eq!(format!("{scale:?}"), "Scale(2 css -> device)");
    }
}
