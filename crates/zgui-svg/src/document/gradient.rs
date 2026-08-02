//! A ramp a document paints with, and the one spread mode that is expressed as a ramp.

use smallvec::SmallVec;

use crate::document::ink::Ink;

/// Which shape a ramp follows.
///
/// Both arms are in the document's own coordinates — the same space the shapes' outlines are in —
/// and both have already absorbed the `gradientTransform` and every group transform above them,
/// so nothing downstream needs a matrix to place a ramp that a shape does not already need.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientKind {
    /// A ramp along the line from `start` to `end`.
    Linear {
        /// Where the ramp begins.
        start: kurbo::Point,
        /// Where it ends.
        end: kurbo::Point,
    },
    /// A ramp outwards from `center` to an ellipse with the given radii.
    ///
    /// Two radii and not one because an SVG `gradientTransform` may scale the two axes differently,
    /// and a circle scaled unevenly is an ellipse.
    Radial {
        /// The centre.
        center: kurbo::Point,
        /// The horizontal radius the ramp reaches its last stop at.
        radius_x: f64,
        /// The vertical radius it reaches its last stop at.
        radius_y: f64,
    },
}

impl GradientKind {
    /// The same ramp shape, reaching twice as far.
    ///
    /// What [`Gradient::reflecting`] needs to turn a reflected spread into a repeating one.
    fn doubled(self) -> Self {
        match self {
            Self::Linear { start, end } => Self::Linear {
                start,
                end: start + (end - start) * 2.0,
            },
            Self::Radial {
                center,
                radius_x,
                radius_y,
            } => Self::Radial {
                center,
                radius_x: radius_x * 2.0,
                radius_y: radius_y * 2.0,
            },
        }
    }
}

/// One colour stop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    /// Where along the ramp it sits, from zero to one.
    pub offset: f32,
    /// What colour it is, which may be the one the element inherits.
    pub color: Ink,
}

/// A ramp between colour stops.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    /// The shape the ramp follows.
    pub kind: GradientKind,
    /// The stops, in increasing offset order.
    pub stops: SmallVec<[Stop; 4]>,
    /// Whether the ramp repeats outside its extent instead of holding its end colours.
    pub repeating: bool,
}

impl Gradient {
    /// A ramp that holds its end colours outside its extent — SVG's `pad`.
    pub fn padded(kind: GradientKind, stops: SmallVec<[Stop; 4]>) -> Self {
        Self {
            kind,
            stops,
            repeating: false,
        }
    }

    /// A ramp that starts over outside its extent — SVG's `repeat`.
    pub fn repeating(kind: GradientKind, stops: SmallVec<[Stop; 4]>) -> Self {
        Self {
            kind,
            stops,
            repeating: true,
        }
    }

    /// A ramp that runs back and forth outside its extent — SVG's `reflect`.
    ///
    /// There is no third spread mode in the model this produces, and there does not need to be: a
    /// reflected ramp is exactly a repeating ramp of twice the extent whose second half is the
    /// first half backwards. Expressing it that way rather than adding a mode is what keeps every
    /// rasteriser able to draw it — one that had never heard of reflection would otherwise draw a
    /// reflected ramp as a repeating one, which is a wrong picture with nothing reporting it.
    ///
    /// ```
    /// use smallvec::smallvec;
    /// use zgui_color::Color;
    /// use zgui_svg::{Gradient, GradientKind, Ink, Stop};
    ///
    /// let red = Ink::Solid(Color::srgb(1.0, 0.0, 0.0, 1.0));
    /// let blue = Ink::Solid(Color::srgb(0.0, 0.0, 1.0, 1.0));
    /// let kind = GradientKind::Linear {
    ///     start: kurbo::Point::new(0.0, 0.0),
    ///     end: kurbo::Point::new(10.0, 0.0),
    /// };
    /// let ramp = Gradient::reflecting(
    ///     kind,
    ///     smallvec![
    ///         Stop { offset: 0.0, color: red },
    ///         Stop { offset: 1.0, color: blue },
    ///     ],
    /// );
    ///
    /// // Twice as far, and back to where it started at the far end.
    /// assert_eq!(
    ///     ramp.kind,
    ///     GradientKind::Linear {
    ///         start: kurbo::Point::new(0.0, 0.0),
    ///         end: kurbo::Point::new(20.0, 0.0),
    ///     }
    /// );
    /// assert!(ramp.repeating);
    /// assert_eq!(ramp.stops.first().map(|stop| stop.color), Some(red));
    /// assert_eq!(ramp.stops.last().map(|stop| stop.color), Some(red));
    /// ```
    pub fn reflecting(kind: GradientKind, stops: SmallVec<[Stop; 4]>) -> Self {
        let mut mirrored: SmallVec<[Stop; 4]> = stops
            .iter()
            .map(|stop| Stop {
                offset: stop.offset / 2.0,
                color: stop.color,
            })
            .collect();
        mirrored.extend(stops.iter().rev().map(|stop| Stop {
            offset: 1.0 - stop.offset / 2.0,
            color: stop.color,
        }));
        Self {
            kind: kind.doubled(),
            stops: mirrored,
            repeating: true,
        }
    }

    /// The same ramp with every stop's alpha scaled by `factor`.
    pub fn faded(mut self, factor: f32) -> Self {
        for stop in &mut self.stops {
            stop.color = stop.color.faded(factor);
        }
        self
    }

    /// Whether any stop takes its colour from the element that draws the document.
    pub fn is_inherited(&self) -> bool {
        self.stops.iter().any(|stop| stop.color.is_inherited())
    }
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;
    use zgui_color::Color;

    use super::{Gradient, GradientKind, Stop};
    use crate::document::ink::Ink;

    fn ramp() -> SmallStops {
        smallvec![
            Stop {
                offset: 0.0,
                color: Ink::Solid(Color::BLACK),
            },
            Stop {
                offset: 1.0,
                color: Ink::Solid(Color::WHITE),
            },
        ]
    }

    type SmallStops = smallvec::SmallVec<[Stop; 2]>;

    #[test]
    fn a_reflected_ramp_is_monotonic_and_symmetric() {
        let stops: smallvec::SmallVec<[Stop; 4]> = ramp().into_iter().collect();
        let reflected = Gradient::reflecting(
            GradientKind::Radial {
                center: kurbo::Point::ZERO,
                radius_x: 4.0,
                radius_y: 8.0,
            },
            stops,
        );
        assert_eq!(
            reflected.kind,
            GradientKind::Radial {
                center: kurbo::Point::ZERO,
                radius_x: 8.0,
                radius_y: 16.0,
            }
        );
        assert!(
            reflected
                .stops
                .windows(2)
                .all(|pair| pair[0].offset <= pair[1].offset),
            "a mirrored ramp must still read forwards: {:?}",
            reflected.stops
        );
        // The turning point is the far end of the original ramp, at the middle of the doubled one.
        let middle = reflected.stops[reflected.stops.len() / 2 - 1];
        assert!((middle.offset - 0.5).abs() < 1.0e-6);
        assert_eq!(middle.color, Ink::Solid(Color::WHITE));
    }

    #[test]
    fn fading_a_ramp_fades_every_stop_and_nothing_else() {
        let stops: smallvec::SmallVec<[Stop; 4]> = ramp().into_iter().collect();
        let kind = GradientKind::Linear {
            start: kurbo::Point::ZERO,
            end: kurbo::Point::new(1.0, 0.0),
        };
        let faded = Gradient::padded(kind, stops).faded(0.5);
        assert_eq!(faded.kind, kind);
        assert!(
            faded
                .stops
                .iter()
                .all(|stop| stop.color.resolve(Color::BLACK).alpha() == 0.5)
        );
    }
}
