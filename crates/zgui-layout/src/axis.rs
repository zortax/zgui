//! The two axes of a box.

/// One of a box's two axes, named physically.
///
/// Physically rather than logically: this engine lays out horizontal writing modes, so the inline
/// axis is the horizontal one and saying so avoids a second vocabulary that means the same thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Axis {
    /// Left to right across the page.
    Horizontal,
    /// Top to bottom down it.
    Vertical,
}

impl Axis {
    /// Both axes, horizontal first.
    pub const BOTH: [Self; 2] = [Self::Horizontal, Self::Vertical];

    /// Where this axis sits in a pair of per-axis values.
    ///
    /// Stated rather than taken from the discriminant, because several places store one value per
    /// axis in a `[T; 2]` and read it back through this: an implicit numbering agrees with
    /// [`Axis::BOTH`] only by coincidence, and the day the two disagree is the day a box is sized
    /// by the measurement it took on the other axis.
    pub const fn index(self) -> usize {
        match self {
            Self::Horizontal => 0,
            Self::Vertical => 1,
        }
    }

    /// The other axis.
    pub const fn other(self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }

    /// A short, stable name, for tree dumps and diagnostics.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Horizontal => "x",
            Self::Vertical => "y",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Axis;

    #[test]
    fn the_other_axis_is_an_involution() {
        for axis in Axis::BOTH {
            assert_eq!(axis.other().other(), axis);
            assert_ne!(axis.other(), axis);
        }
    }
}
