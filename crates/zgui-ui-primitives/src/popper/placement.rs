//! Where a floating surface goes relative to what it is anchored to.

/// Which side of the anchor the surface sits on.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Side {
    /// Above the anchor.
    Top,
    /// To the right of it.
    Right,
    /// Below it.
    #[default]
    Bottom,
    /// To the left of it.
    Left,
}

impl Side {
    /// Every side, clockwise from the top.
    pub const ALL: &'static [Self] = &[Self::Top, Self::Right, Self::Bottom, Self::Left];

    /// The side directly across the anchor, which is what flipping picks.
    pub const fn opposite(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// Whether this side puts the surface above or below rather than beside.
    pub const fn is_vertical(self) -> bool {
        matches!(self, Self::Top | Self::Bottom)
    }

    /// How this is written as an attribute value, which is what a style sheet selects on to point
    /// an arrow and to choose an entry animation.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Left => "left",
        }
    }
}

/// Where along the anchor's edge the surface is lined up.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Align {
    /// Flush with the anchor's leading edge.
    Start,
    /// Centred on the anchor.
    #[default]
    Center,
    /// Flush with the anchor's trailing edge.
    End,
}

impl Align {
    /// Every alignment, in reading order.
    pub const ALL: &'static [Self] = &[Self::Start, Self::Center, Self::End];

    /// How this is written as an attribute value.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Center => "center",
            Self::End => "end",
        }
    }
}

/// A side and an alignment: the whole of where a floating surface is asked to go.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Placement {
    /// Which side of the anchor.
    pub side: Side,
    /// Where along that side.
    pub align: Align,
}

impl Placement {
    /// Directly below the anchor, centred: what a popover asks for unless it says otherwise.
    pub const BOTTOM: Self = Self::new(Side::Bottom, Align::Center);
    /// Directly above the anchor, centred: what a tooltip over a toolbar asks for.
    pub const TOP: Self = Self::new(Side::Top, Align::Center);

    /// A placement.
    pub const fn new(side: Side, align: Align) -> Self {
        Self { side, align }
    }

    /// The same alignment on the opposite side.
    pub const fn flipped(self) -> Self {
        Self::new(self.side.opposite(), self.align)
    }
}

#[cfg(test)]
mod tests {
    use super::{Align, Placement, Side};

    #[test]
    fn flipping_crosses_the_anchor_and_keeps_the_alignment() {
        // The alignment must survive: a menu flush with its trigger's left edge that flipped
        // upwards and also jumped to the right edge would look like it had moved for no reason.
        let placement = Placement::new(Side::Bottom, Align::Start);
        assert_eq!(placement.flipped(), Placement::new(Side::Top, Align::Start));
        assert_eq!(placement.flipped().flipped(), placement);
    }

    #[test]
    fn every_side_and_every_alignment_has_a_distinct_attribute_value() {
        let mut sides: Vec<&str> = Side::ALL.iter().map(|side| side.name()).collect();
        sides.sort_unstable();
        sides.dedup();
        assert_eq!(sides.len(), Side::ALL.len());

        let mut aligns: Vec<&str> = Align::ALL.iter().map(|align| align.name()).collect();
        aligns.sort_unstable();
        aligns.dedup();
        assert_eq!(aligns.len(), Align::ALL.len());
    }

    #[test]
    fn the_two_axes_are_the_two_pairs() {
        assert!(Side::Top.is_vertical() && Side::Bottom.is_vertical());
        assert!(!Side::Left.is_vertical() && !Side::Right.is_vertical());
        for side in Side::ALL {
            assert_eq!(side.opposite().opposite(), *side);
            assert_eq!(side.is_vertical(), side.opposite().is_vertical());
        }
    }
}
