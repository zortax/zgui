//! Whether the document under a differential contains a gradient that moves.
//!
//! A differential can only find a defect in a document that contains one. Every comparison this
//! harness runs was, until the probe row existed, over documents in which no gradient ever changed
//! position — so the arm of the paint path that samples a ramp against where the box was *painted*
//! rather than against where the box *is* was never exercised by any of them, and all five were
//! green while the defect was on the screen.
//!
//! This is the non-vacuity assertion for that. It is not the comparison and it does not make one:
//! it says only that the document a comparison is about has a gradient-filled box in it and that
//! the box moved during the script, which is the precondition without which the comparison's
//! verdict means nothing either way.
//!
//! It reads the recorded display list rather than the window's live one, and that is deliberate:
//! the recorded list is the very text the comparison is made from, so a gradient covered here is a
//! gradient the comparison saw.

/// How a gradient fill is spelled in a recorded display list.
const RAMPS: [&str; 2] = ["fill=linear", "fill=radial"];

/// What the script saw of the document's gradients.
#[derive(Default)]
pub(crate) struct Coverage {
    /// Every distinct rectangle a gradient-filled box was drawn in, in the order first seen.
    seen: Vec<String>,
    /// How many gradient-filled quads have been sampled altogether.
    samples: usize,
}

impl Coverage {
    /// Records every gradient-filled box in one recorded display list.
    pub(crate) fn sample(&mut self, recorded: &str) {
        for line in recorded.lines() {
            if !RAMPS.iter().any(|ramp| line.contains(ramp)) {
                continue;
            }
            let Some(at) = bounds_of(line) else { continue };
            self.samples += 1;
            if !self.seen.contains(&at) {
                self.seen.push(at);
            }
        }
    }

    /// Whether the document drew a gradient-filled box at all.
    pub(crate) fn found_one(&self) -> bool {
        self.samples > 0
    }

    /// Whether one of them was drawn in more than one place.
    pub(crate) fn one_moved(&self) -> bool {
        self.seen.len() > 1
    }

    /// How it reads in a report.
    pub(crate) fn describe(&self) -> String {
        format!(
            "{} gradient-filled quads across {} distinct rectangles",
            self.samples,
            self.seen.len()
        )
    }

    /// Fails the run when the document cannot exercise the paint path a comparison is about.
    ///
    /// # Panics
    ///
    /// Panics when no gradient was drawn, or when every one of them stood still. Either is a
    /// document in which the comparison this coverage belongs to would pass whatever the paint path
    /// did, and a comparison that cannot fail is not evidence that anything works.
    pub(crate) fn assert_non_vacuous(&self, size: &str) {
        assert!(
            self.found_one(),
            "the document at {size} draws no gradient-filled box, so every comparison over it is \
             silent about the arm of the paint path that samples a ramp"
        );
        assert!(
            self.one_moved(),
            "the document at {size} draws a gradient-filled box and it never moved, so the \
             comparison cannot tell a ramp that travels with its box from one that does not: {}",
            self.describe()
        );
    }
}

/// The `bounds=rect(...)` a display-list line names, as it is spelled.
fn bounds_of(line: &str) -> Option<String> {
    let rest = line.split_once("bounds=")?.1;
    let end = rest.find(')')?;
    Some(rest[..=end].to_owned())
}

#[cfg(test)]
mod tests {
    use super::Coverage;

    /// One quad line, filled with a ramp, at `y`.
    fn ramp_at(y: u32) -> String {
        format!("  quad order=4 bounds=rect(24, {y}, 96, 64) fill=linear from=(0, 0) to=(1, 1)\n")
    }

    #[test]
    fn a_document_with_no_gradient_in_it_is_refused() {
        let mut coverage = Coverage::default();
        coverage.sample("  quad order=1 bounds=rect(0, 0, 8, 8) fill=solid srgb(0, 0, 0, 1)\n");
        assert!(!coverage.found_one());
    }

    #[test]
    fn a_gradient_that_never_moves_is_refused() {
        let mut coverage = Coverage::default();
        for _ in 0..4 {
            coverage.sample(&ramp_at(68));
        }
        assert!(coverage.found_one());
        assert!(!coverage.one_moved(), "{}", coverage.describe());
    }

    #[test]
    fn a_gradient_that_moves_is_accepted() {
        let mut coverage = Coverage::default();
        coverage.sample(&ramp_at(68));
        coverage.sample(&ramp_at(140));
        assert!(
            coverage.found_one() && coverage.one_moved(),
            "{}",
            coverage.describe()
        );
    }
}
