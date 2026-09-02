//! What shape a box's corners are cut to.
//!
//! # One number for every shape
//!
//! A corner is a superellipse quadrant: `|x/rx|^n + |y/ry|^n = 1`. The exponent is the whole of the
//! difference between the shapes CSS names, which is why this is one number rather than an
//! enumeration with a case per name:
//!
//! | exponent | shape |
//! |---|---|
//! | below one | scooped — the corner is cut *inwards* |
//! | one | bevelled — a straight chamfer, because `\|x/rx\| + \|y/ry\| = 1` is a line |
//! | two | round — the ellipse quadrant `border-radius` has always meant |
//! | four | squircle |
//! | large | square — the corner closes up on the radius box |
//!
//! Two follows from the others rather than being special-cased, and that matters: an exponent of
//! two has to draw *exactly* what the elliptical path drew before this existed, or every rounded
//! box in every document shifts by a fraction of a pixel. It is kept as its own branch in the
//! shading code for that reason, not as an optimisation.

use bytemuck::{Pod, Zeroable};

/// The exponent a box's corners are cut with.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct CornerShape(pub f32);

impl Default for CornerShape {
    fn default() -> Self {
        Self::ROUND
    }
}

impl CornerShape {
    /// The ellipse quadrant, which is what a corner radius has always drawn.
    pub const ROUND: Self = Self(2.0);

    /// A straight chamfer.
    pub const BEVEL: Self = Self(1.0);

    /// The smoothed corner, cut fuller than a circle.
    pub const SQUIRCLE: Self = Self(4.0);

    /// A corner cut inwards.
    pub const SCOOP: Self = Self(0.5);

    /// A corner so full it closes on the radius box, which is a square corner drawn the long way.
    ///
    /// Not infinity: the shading code raises a coordinate to this power, and an infinite exponent
    /// makes every point on an axis produce a value nothing can be compared against.
    pub const NOTCH: Self = Self(64.0);

    /// The shape `exponent` names, kept inside what the shading code can evaluate.
    ///
    /// Zero and below would invert the corner rather than scooping it, and an exponent past the
    /// notch draws the same square corner as the notch does.
    pub fn new(exponent: f32) -> Self {
        Self(exponent.clamp(0.01, Self::NOTCH.0))
    }

    /// Whether this is the ellipse every corner radius drew before shapes existed.
    ///
    /// The one question the shading code and the emit path both ask, because the answer decides
    /// whether anything about a box is different from what it always was.
    pub fn is_round(self) -> bool {
        self.0 == Self::ROUND.0
    }

    /// The exponent itself, for an instance field that carries it.
    pub const fn get(self) -> f32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::CornerShape;

    /// The default has to be the ellipse, or every existing document changes shape.
    #[test]
    fn the_default_is_the_ellipse_a_corner_radius_always_drew() {
        assert_eq!(CornerShape::default(), CornerShape::ROUND);
        assert!(CornerShape::default().is_round());
    }

    #[test]
    fn every_named_shape_but_round_is_something_else() {
        for shape in [
            CornerShape::BEVEL,
            CornerShape::SQUIRCLE,
            CornerShape::SCOOP,
            CornerShape::NOTCH,
        ] {
            assert!(!shape.is_round(), "{shape:?}");
        }
    }

    /// An exponent the shading code cannot evaluate is brought back to one it can, rather than
    /// reaching a shader as an infinity or a negative power.
    #[test]
    fn an_exponent_outside_what_can_be_drawn_is_brought_inside_it() {
        assert!(CornerShape::new(0.0).get() > 0.0);
        assert!(CornerShape::new(-4.0).get() > 0.0);
        assert_eq!(CornerShape::new(1.0e9), CornerShape::NOTCH);
        assert_eq!(CornerShape::new(f32::INFINITY), CornerShape::NOTCH);
    }

    #[test]
    fn an_ordinary_exponent_is_kept_as_it_was_written() {
        assert_eq!(CornerShape::new(4.0), CornerShape::SQUIRCLE);
        assert_eq!(CornerShape::new(2.5).get(), 2.5);
    }
}
