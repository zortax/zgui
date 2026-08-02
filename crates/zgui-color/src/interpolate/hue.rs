//! How an interpolation crosses the hue circle.
//!
//! Hue is an angle, so "half-way between 20° and 320°" has two answers — 170°, the long way round
//! through green, and 350°, the short way through red — and neither is more correct than the
//! other. CSS lets the author choose, and the choice is expressed as a fixup applied to the two
//! endpoints before they are interpolated as ordinary numbers.

/// Which way round the hue circle an interpolation travels.
///
/// ```
/// use zgui_color::HueInterpolation;
///
/// // From 20° to 320°: the short way goes backwards through red.
/// assert_eq!(HueInterpolation::Shorter.fixup(20.0, 320.0), (380.0, 320.0));
/// // The long way goes forwards through green.
/// assert_eq!(HueInterpolation::Longer.fixup(20.0, 320.0), (20.0, 320.0));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HueInterpolation {
    /// Take whichever arc is shorter, which is what an author gets by default.
    #[default]
    Shorter,
    /// Take whichever arc is longer.
    Longer,
    /// Travel in the direction of increasing hue, however far that is.
    Increasing,
    /// Travel in the direction of decreasing hue, however far that is.
    Decreasing,
}

impl HueInterpolation {
    /// Adjusts a pair of hues, in degrees, so that interpolating between them as plain numbers
    /// travels the intended way round the circle.
    ///
    /// Either returned hue may fall outside `0..360`; that is the point. The angle `380°` is the
    /// same direction as `20°`, but interpolating towards it from `320°` moves forwards rather
    /// than backwards.
    pub fn fixup(self, from: f32, to: f32) -> (f32, f32) {
        let difference = to - from;
        match self {
            Self::Shorter => {
                if difference > 180.0 {
                    (from + 360.0, to)
                } else if difference < -180.0 {
                    (from, to + 360.0)
                } else {
                    (from, to)
                }
            }
            Self::Longer => {
                if difference > 0.0 && difference < 180.0 {
                    (from + 360.0, to)
                } else if difference > -180.0 && difference <= 0.0 {
                    (from, to + 360.0)
                } else {
                    (from, to)
                }
            }
            Self::Increasing => {
                if difference < 0.0 {
                    (from, to + 360.0)
                } else {
                    (from, to)
                }
            }
            Self::Decreasing => {
                if difference > 0.0 {
                    (from + 360.0, to)
                } else {
                    (from, to)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HueInterpolation;

    /// The midpoint of a fixed-up pair, brought back into `0..360`.
    fn midpoint(method: HueInterpolation, from: f32, to: f32) -> f32 {
        let (from, to) = method.fixup(from, to);
        crate::convert::polar::normalize_hue((from + to) / 2.0)
    }

    #[test]
    fn the_short_way_and_the_long_way_are_opposite() {
        assert!((midpoint(HueInterpolation::Shorter, 20.0, 320.0) - 350.0).abs() < 1e-4);
        assert!((midpoint(HueInterpolation::Longer, 20.0, 320.0) - 170.0).abs() < 1e-4);
    }

    #[test]
    fn direction_is_respected_however_far_it_is() {
        assert!((midpoint(HueInterpolation::Increasing, 20.0, 320.0) - 170.0).abs() < 1e-4);
        assert!((midpoint(HueInterpolation::Decreasing, 20.0, 320.0) - 350.0).abs() < 1e-4);
        assert!((midpoint(HueInterpolation::Increasing, 320.0, 20.0) - 350.0).abs() < 1e-4);
        assert!((midpoint(HueInterpolation::Decreasing, 320.0, 20.0) - 170.0).abs() < 1e-4);
    }

    #[test]
    fn a_shorter_crossing_of_zero_goes_the_short_way() {
        assert!((midpoint(HueInterpolation::Shorter, 350.0, 10.0) - 0.0).abs() < 1e-4);
        assert!((midpoint(HueInterpolation::Shorter, 10.0, 350.0) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn equal_hues_stay_put_except_the_long_way_round() {
        assert_eq!(HueInterpolation::Shorter.fixup(90.0, 90.0), (90.0, 90.0));
        assert_eq!(HueInterpolation::Increasing.fixup(90.0, 90.0), (90.0, 90.0));
        assert_eq!(HueInterpolation::Decreasing.fixup(90.0, 90.0), (90.0, 90.0));
        // Going the long way between two identical hues is a full turn, not a standstill.
        assert_eq!(HueInterpolation::Longer.fixup(90.0, 90.0), (90.0, 450.0));
    }
}
