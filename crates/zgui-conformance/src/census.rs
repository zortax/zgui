//! Every longhand the engine generated, and what this framework says about each of them.
//!
//! # The denominator
//!
//! The engine writes out the list of properties it was built with, and that list — not a
//! hand-written one — is what a parity fraction is taken over. It holds alias spellings beside
//! canonical ones, because an author may write either, and an alias is classified by whatever
//! classifies its target.
//!
//! # Why a merge needs a policy
//!
//! Declarations live beside the code that reads each property, which is the only arrangement that
//! survives a refactor — and it means two crates may both declare the same property, for their own
//! reasons, and disagree. One crate reading a value while another only hashes it is not a
//! contradiction to resolve by picking a winner at random: the property *is* consumed, by the crate
//! that consumes it. So the merge takes the strongest answer and keeps the disagreement, which
//! [`Census::disagreements`] reports and the parity document prints.
//!
//! A *disagreement* is two crates giving the property two different **treatments**, not two crates
//! naming two different modules. Nine longhands are read by both the style engine and the text
//! stack, and each of those is one property with two readers rather than a dispute — recording them
//! would fill the report's disagreement table with rows whose two columns read the same word and
//! bury the one row that is a real dispute.
//!
//! ```
//! use zgui_conformance::census::Census;
//!
//! let census = Census::take();
//! assert_eq!(census.unclassified(), Vec::<String>::new());
//! assert_eq!(census.classified(), census.canonical().len());
//! ```

use std::collections::BTreeMap;

use zgui_css::parity::{Registration, Support, catalog};

/// What this framework says about every longhand the engine generated.
#[derive(Clone, Debug)]
pub struct Census {
    /// One answer per canonical longhand.
    answers: BTreeMap<String, Registration>,
    /// The canonical longhands, in order.
    canonical: Vec<String>,
    /// Every longhand name the engine generated, alias spellings included.
    names: Vec<zgui_css::parity::Longhand>,
    /// Where two crates answered differently for one property.
    disagreements: Vec<Disagreement>,
}

/// Two crates answering differently about one property.
///
/// Named here as well as where it is defined, so that a reader of the census never has to leave
/// this module to find out what one of these holds.
pub use zgui_css::parity::Disagreement;

impl Census {
    /// Takes the census against the engine as this framework configures it.
    pub fn take() -> Self {
        zgui_css::enable_css_features();

        let mut answers: BTreeMap<String, Registration> = BTreeMap::new();
        let mut disagreements = Vec::new();
        for registration in crate::registrations() {
            let name = registration.css_name();
            match answers.get(&name) {
                None => {
                    answers.insert(name, registration);
                }
                Some(existing)
                    if existing.support().strength() == registration.support().strength() => {}
                Some(existing) => {
                    let (kept, dropped) =
                        if registration.support().strength() > existing.support().strength() {
                            (registration, *existing)
                        } else {
                            (*existing, registration)
                        };
                    disagreements.push(Disagreement {
                        css_name: name.clone(),
                        kept: kept.support(),
                        dropped: dropped.support(),
                    });
                    answers.insert(name, kept);
                }
            }
        }
        disagreements.sort_by(|left, right| left.css_name.cmp(&right.css_name));

        Self {
            answers,
            canonical: catalog::canonical_longhands(),
            names: catalog::longhands(),
            disagreements,
        }
    }

    /// The canonical longhands, which is what there is one answer per.
    pub fn canonical(&self) -> &[String] {
        &self.canonical
    }

    /// Every name the engine generated, alias spellings included.
    pub fn names(&self) -> &[zgui_css::parity::Longhand] {
        &self.names
    }

    /// The answer for one property, by the name a style sheet writes.
    pub fn answer(&self, css_name: &str) -> Option<Support> {
        self.answers.get(css_name).map(Registration::support)
    }

    /// Every canonical longhand nobody has classified.
    ///
    /// This is the question the parity gate asks. A property with no answer is not a property that
    /// works: from the outside it is indistinguishable from one that does, right up to the moment
    /// an author writes it.
    pub fn unclassified(&self) -> Vec<String> {
        self.canonical
            .iter()
            .filter(|name| !self.answers.contains_key(*name))
            .cloned()
            .collect()
    }

    /// Every classified longhand that the engine did not generate.
    ///
    /// The other direction, and the one a register drifts in: a row for a property that used to
    /// exist, or that was misspelled, is a row that will never be checked against anything.
    pub fn unrecognised(&self) -> Vec<String> {
        self.answers
            .keys()
            .filter(|name| {
                !self.canonical.contains(*name)
                    && !matches!(
                        self.answers.get(*name).map(Registration::support),
                        Some(Support::Absent(_)),
                    )
            })
            .cloned()
            .collect()
    }

    /// How many canonical longhands have an answer.
    pub fn classified(&self) -> usize {
        self.canonical
            .iter()
            .filter(|name| self.answers.contains_key(*name))
            .count()
    }

    /// How many canonical longhands some module reads the value of.
    pub fn implemented(&self) -> usize {
        self.by(|support| support.is_consumed())
    }

    /// How many parse and cascade with nothing reading them.
    pub fn ignored(&self) -> usize {
        self.by(|support| matches!(support, Support::Ignored(_)))
    }

    /// How many are not available from the engine at all.
    pub fn absent(&self) -> usize {
        self.by(|support| matches!(support, Support::Absent(_)))
    }

    /// Where two crates answered differently.
    pub fn disagreements(&self) -> &[Disagreement] {
        &self.disagreements
    }

    /// Every declaration the engine now contradicts.
    pub fn stale(&self) -> Vec<zgui_css::parity::ParityError> {
        self.answers
            .values()
            .filter_map(|registration| registration.check().err())
            .collect()
    }

    /// Counts the canonical longhands whose answer satisfies `wanted`.
    fn by(&self, wanted: impl Fn(Support) -> bool) -> usize {
        self.canonical
            .iter()
            .filter_map(|name| self.answers.get(name))
            .filter(|registration| wanted(registration.support()))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::Census;

    /// Every longhand the engine generated has an answer, and every answer names a real longhand.
    #[test]
    fn the_census_is_complete_in_both_directions() {
        let census = Census::take();
        assert_eq!(
            census.unclassified(),
            Vec::<String>::new(),
            "a longhand nobody has classified",
        );
        assert_eq!(
            census.unrecognised(),
            Vec::<String>::new(),
            "a declaration for a longhand this build does not generate",
        );
        assert_eq!(census.classified(), census.canonical().len());
    }

    /// No declaration anywhere contradicts the engine it describes.
    #[test]
    fn no_declaration_has_gone_stale() {
        let census = Census::take();
        assert_eq!(census.stale(), Vec::new(), "{:?}", census.stale());
    }

    /// A disagreement is two treatments, and every recorded one really is two.
    ///
    /// Both halves. The first is what the report's table claims about itself: a row whose two
    /// columns say the same word is not a disagreement, and printing one is how the single real
    /// dispute in this workspace got lost among eight properties that merely have two readers. The
    /// second is the control — a register where every property agreed would satisfy the first half
    /// while proving that the detector is switched off.
    #[test]
    fn a_recorded_disagreement_is_two_different_treatments() {
        let census = Census::take();
        for row in census.disagreements() {
            assert_ne!(
                row.kept.strength(),
                row.dropped.strength(),
                "`{}` is recorded as a disagreement between two identical treatments",
                row.css_name,
            );
            assert!(row.kept.is_consumed() || !row.dropped.is_consumed());
        }
        assert!(
            !census.disagreements().is_empty(),
            "the workspace has at least one property one crate reads and another only hashes; a \
             detector that found none of them would be reporting its own silence",
        );
    }

    /// The denominator is the real one, and the fraction is not perfect.
    ///
    /// A census that had somehow read an empty property list would classify everything and report
    /// full parity, which is the failure this instrument exists to prevent — so both ends are
    /// asserted rather than only the one that looks good.
    #[test]
    fn the_denominator_and_the_numerator_are_both_real() {
        let census = Census::take();
        assert!(
            census.canonical().len() > 200,
            "{}",
            census.canonical().len()
        );
        assert!(census.names().len() > census.canonical().len());
        assert!(census.implemented() > 0);
        assert!(
            census.implemented() < census.canonical().len(),
            "a claim of complete parity would need evidence this build cannot produce",
        );
        assert_eq!(
            census.implemented() + census.ignored() + census.absent(),
            census.classified(),
        );
    }
}
