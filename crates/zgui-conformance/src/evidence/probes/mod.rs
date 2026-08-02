//! One probe declaration for every longhand the engine generates.
//!
//! The point of covering *all* of them rather than only the ones some crate claims is that the two
//! failures are symmetric. A property declared implemented that changes nothing is an over-claim; a
//! property nobody declared that changes the fragment tree is an under-claim, and it is the one an
//! author meets as a working feature the parity number does not admit to. Probing everything is
//! what makes both of them answerable.
//!
//! The grouping is by what the property is about, so a group's file stays short enough to read.

pub mod geometry;
pub mod motion;
pub mod text;
pub mod visual;

use crate::evidence::probe::Probe;

/// Every probe, in one list.
pub fn all() -> Vec<Probe> {
    [
        geometry::PROBES,
        visual::PROBES,
        text::PROBES,
        motion::PROBES,
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use zgui_css::parity::catalog;

    /// There is exactly one probe per longhand the engine generates, and no probe for anything
    /// else.
    ///
    /// Both halves matter. A missing probe is a property whose classification rests on nothing; a
    /// probe for a property that no longer exists is a row that will never run again, and a table
    /// that quietly shrank would make every remaining answer look complete.
    #[test]
    fn the_probes_and_the_engines_longhands_are_the_same_set() {
        zgui_css::enable_css_features();
        let probed: BTreeSet<String> = super::all().iter().map(|probe| probe.css_name()).collect();
        let generated: BTreeSet<String> = catalog::canonical_longhands().into_iter().collect();

        let unprobed: Vec<&String> = generated.difference(&probed).collect();
        let extra: Vec<&String> = probed.difference(&generated).collect();
        assert_eq!(unprobed, Vec::<&String>::new(), "longhands with no probe");
        assert_eq!(extra, Vec::<&String>::new(), "probes for no longhand");
        assert_eq!(probed.len(), super::all().len(), "a longhand probed twice");
    }
}
