//! Rule 1, made mechanical: how many scattered dirty siblings a walk-budget fixture may have.

/// The dirty siblings of one parent in a fixture, and whether they are a shape a walk budget may be
/// written over.
///
/// The structure being budgeted holds four exact children and degrades to an inclusive span on the
/// fifth: four scattered marks over ten thousand children cost four probes, five cost nine thousand
/// nine hundred and ninety-seven. A budget written over the second shape measures the degradation
/// and not the design.
///
/// A contiguous run is fine at any length, because a span over a run *is* the run.
///
/// ```
/// use zgui_testkit_scene::fixture::walk::SiblingMarks;
///
/// // Four scattered marks: a budget over this measures the exact-children path.
/// assert!(SiblingMarks::new(10_000, [0, 100, 900, 4_000]).is_ok());
///
/// // Five contiguous: still fine, because the span is the run.
/// assert!(SiblingMarks::new(10_000, [7, 8, 9, 10, 11]).is_ok());
///
/// // Five scattered: refused.
/// assert!(SiblingMarks::new(10_000, [0, 100, 900, 4_000, 9_000]).is_err());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiblingMarks {
    /// How many children the parent has.
    count: usize,
    /// Which of them are marked, ascending and deduplicated.
    marked: Vec<usize>,
}

/// Why a fixture was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FixtureError {
    /// A mark named a child the parent does not have.
    OutOfRange {
        /// The index that was named.
        index: usize,
        /// How many children there are.
        count: usize,
    },
    /// More scattered marks than the exact-children path holds.
    TooScattered {
        /// How many marks there are.
        marks: usize,
        /// How many scattered marks are allowed.
        limit: usize,
    },
}

impl core::fmt::Display for FixtureError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutOfRange { index, count } => write!(
                formatter,
                "child {index} does not exist: the parent has {count}"
            ),
            Self::TooScattered { marks, limit } => write!(
                formatter,
                "{marks} scattered dirty siblings, at most {limit} or a contiguous run. Past the \
                 limit the dirty-children structure degrades from exact children to an inclusive \
                 span, and a walk budget written here would measure the span — roughly two and a \
                 half thousand times the probes — rather than the design. Use a contiguous run, or \
                 write the larger budget deliberately with the reason beside it."
            ),
        }
    }
}

impl core::error::Error for FixtureError {}

impl SiblingMarks {
    /// How many scattered marks the exact-children path holds.
    pub const MAX_SCATTERED: usize = 4;

    /// The marks over a parent with `count` children, checked against rule 1.
    ///
    /// # Errors
    ///
    /// [`FixtureError::OutOfRange`] when a mark names a child that does not exist, and
    /// [`FixtureError::TooScattered`] when there are more scattered marks than the exact-children
    /// path holds.
    pub fn new(
        count: usize,
        marked: impl IntoIterator<Item = usize>,
    ) -> Result<Self, FixtureError> {
        let mut marked: Vec<usize> = marked.into_iter().collect();
        marked.sort_unstable();
        marked.dedup();
        for index in &marked {
            if *index >= count {
                return Err(FixtureError::OutOfRange {
                    index: *index,
                    count,
                });
            }
        }
        let fixture = Self { count, marked };
        if fixture.marked.len() > Self::MAX_SCATTERED && !fixture.is_contiguous() {
            return Err(FixtureError::TooScattered {
                marks: fixture.marked.len(),
                limit: Self::MAX_SCATTERED,
            });
        }
        Ok(fixture)
    }

    /// How many children the parent has.
    pub fn count(&self) -> usize {
        self.count
    }

    /// The marked children, ascending.
    pub fn marked(&self) -> &[usize] {
        &self.marked
    }

    /// Whether the marks are one unbroken run.
    pub fn is_contiguous(&self) -> bool {
        self.marked.windows(2).all(|pair| pair[1] == pair[0] + 1)
    }

    /// How many probes servicing these marks costs, under the structure rule 1 describes.
    ///
    /// Exact children are probed one apiece; a span is probed across its whole inclusive extent.
    /// This is what makes the rule a number rather than a warning.
    pub fn probes(&self) -> usize {
        if self.marked.is_empty() {
            return 0;
        }
        if self.marked.len() <= Self::MAX_SCATTERED {
            return self.marked.len();
        }
        let first = self.marked[0];
        let last = self.marked[self.marked.len() - 1];
        last - first + 1
    }
}

#[cfg(test)]
mod tests {
    use super::{FixtureError, SiblingMarks};

    #[test]
    fn four_scattered_marks_cost_four_probes() {
        let marks = SiblingMarks::new(10_000, [0, 100, 900, 4_000]).expect("within the limit");
        assert_eq!(marks.probes(), 4);
        assert!(!marks.is_contiguous());
    }

    #[test]
    fn the_fifth_scattered_mark_is_refused_and_the_message_says_why() {
        let error = SiblingMarks::new(10_000, [0, 100, 900, 4_000, 9_000])
            .expect_err("the fifth mark degrades the structure");
        assert_eq!(error, FixtureError::TooScattered { marks: 5, limit: 4 });
        assert!(error.to_string().contains("contiguous run"));
    }

    #[test]
    fn a_contiguous_run_is_allowed_at_any_length_and_costs_its_length() {
        let marks = SiblingMarks::new(10_000, 7..=5_006).expect("a run is a run at any length");
        assert!(marks.is_contiguous());
        assert_eq!(marks.probes(), 5_000);
    }

    #[test]
    fn the_cost_the_rule_is_written_against_is_the_one_it_reports() {
        // The measurement rule 1 rests on: four scattered marks cost four probes, and five cost
        // very nearly the whole span between the outermost two. A fixture builder that could not
        // tell those apart would be a rule with nothing behind it.
        let four = SiblingMarks::new(10_000, [0, 1, 2, 9_996]).expect("within the limit");
        assert_eq!(four.probes(), 4);

        let five = SiblingMarks {
            count: 10_000,
            marked: vec![0, 1, 2, 9_996, 9_997],
        };
        assert_eq!(five.probes(), 9_998);
    }

    #[test]
    fn a_mark_outside_the_parent_is_refused() {
        assert_eq!(
            SiblingMarks::new(4, [7]).expect_err("child 7 does not exist"),
            FixtureError::OutOfRange { index: 7, count: 4 }
        );
    }

    #[test]
    fn repeated_marks_are_one_mark() {
        let marks = SiblingMarks::new(10, [1, 1, 1, 1, 1, 1]).expect("one child, marked often");
        assert_eq!(marks.marked(), [1]);
        assert_eq!(marks.probes(), 1);
    }
}
