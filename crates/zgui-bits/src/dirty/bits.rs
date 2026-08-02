//! The invalidation lattice: which kinds of work a node still owes.

bitflags::bitflags! {
    /// Work a node still owes before the next frame can be presented.
    ///
    /// The same bit positions describe a node's own obligations and, shifted left by 32, the union
    /// of its descendants' obligations. [`DirtyCell`](crate::DirtyCell) is what stores the pair.
    ///
    /// The flags are a lattice under union: a node owing both a restyle and a repaint carries both
    /// bits, and marking a node twice with the same bit is idempotent. Nothing here implies an
    /// order between two bits — the order is the frame pipeline's, and each bit is retired by
    /// exactly the stage that services it.
    ///
    /// ```
    /// use zgui_bits::Dirty;
    ///
    /// let work = Dirty::RESTYLE | Dirty::REPAINT;
    /// assert!(work.contains(Dirty::REPAINT));
    /// assert!(!work.contains(Dirty::RELAYOUT));
    /// assert_eq!(work | Dirty::RESTYLE, work);
    /// ```
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug)]
    pub struct Dirty: u32 {
        /// Selector matching must run again for this element.
        const RESTYLE      = 1 << 0;
        /// Only the cascade must run again; selector matches are still valid.
        const RECASCADE    = 1 << 1;
        /// The box tree fragment rooted here must be rebuilt.
        const REBUILD_BOX  = 1 << 2;
        /// Layout inputs changed; this box's measure and arrange are invalid.
        const RELAYOUT     = 1 << 3;
        /// Text content or a shaping-relevant style changed.
        const RESHAPE      = 1 << 4;
        /// Only a break- or align-relevant style, or the available width, changed.
        const REBREAK      = 1 << 5;
        /// Absolute position changed but size and content did not.
        const REPOSITION   = 1 << 6;
        /// This node's paint output changed.
        const REPAINT      = 1 << 7;
        /// The child list changed: insert, remove or reorder.
        const CHILDREN     = 1 << 8;
        /// The accessibility projection of this node changed.
        const A11Y         = 1 << 9;
        /// The hit-test entry for this node changed.
        const REHIT        = 1 << 10;
        /// A scroll offset under this node changed.
        const SCROLL       = 1 << 11;
        /// This fragment's transform, ink rectangle or scrollable overflow changed; its size,
        /// position and content did not, so layout need not run again.
        const REFRAGMENT   = 1 << 12;
        /// Stacking-context membership or paint order changed.
        const RESTACK      = 1 << 13;
        /// A transition or animation is running on this node, so the next frame has a deadline.
        const ANIMATING    = 1 << 14;
    }
}

impl Dirty {
    /// Whether this set records no obligation at all.
    ///
    /// ```
    /// use zgui_bits::Dirty;
    ///
    /// assert!(Dirty::empty().is_clean());
    /// assert!(!Dirty::RESHAPE.is_clean());
    /// ```
    pub const fn is_clean(self) -> bool {
        self.bits() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::Dirty;

    #[test]
    fn every_flag_fits_the_low_half_of_a_word() {
        assert_eq!(Dirty::all().bits() & 0xffff_8000, 0);
    }

    #[test]
    fn the_lattice_has_fifteen_distinct_bits() {
        assert_eq!(Dirty::all().bits().count_ones(), 15);
    }

    #[test]
    fn union_is_idempotent_and_commutative() {
        let a = Dirty::RESTYLE | Dirty::A11Y;
        let b = Dirty::A11Y | Dirty::SCROLL;
        assert_eq!(a | a, a);
        assert_eq!(a | b, b | a);
        assert_eq!((a | b) & a, a);
    }
}
