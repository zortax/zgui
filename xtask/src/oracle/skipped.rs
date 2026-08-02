//! How many steps a differential is allowed to decline to compare.
//!
//! A step whose two windows have already laid the document out differently is skipped, because what
//! it would report is a fact about the layout they arrived at rather than about what either of them
//! answers. That is the right thing to do with such a step and the wrong thing to leave unbounded:
//! the run's own floor is that it compared more steps than it skipped, so a document that quietly
//! went from declining ten of ninety-five to declining forty-seven would still be green, and the
//! gate would be making a much smaller claim without saying so.
//!
//! So the count is recorded per document, exactly, and read back. It moves when the two windows
//! start or stop agreeing about the layout, which is a real change and one somebody should have to
//! write down — in either direction, because a run that skips *fewer* steps than recorded is a run
//! whose record is stale, and a stale record is how a number stops meaning anything.

use crate::error::{Error, Result};

/// The line a differential prints its tally on, and the field this reads out of it.
const FIELD: &str = "apart=";

/// How many steps `output` reported declining to compare.
///
/// `None` when the run printed no tally at all, which is a different failure and is reported by the
/// criterion check rather than here.
pub(crate) fn reported(output: &str) -> Option<usize> {
    output.lines().find_map(|line| {
        let at = line.find(FIELD)?;
        let rest = &line[at + FIELD.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

/// Checks the count against what this document is recorded as declining.
///
/// # Errors
///
/// Fails when the run declined a different number of steps than recorded, in either direction.
pub(crate) fn check(
    gate: &str,
    size: &str,
    expected: usize,
    output: &str,
    here: &str,
) -> Result<()> {
    let Some(found) = reported(output) else {
        return Err(Error::failed(format!(
            "the {gate} run at {size} printed no tally, so how much of the script it actually \
             compared is unknown"
        )));
    };
    if found == expected {
        return Ok(());
    }
    let direction = if found > expected {
        "declines more of the script than it did, so the gate is making a smaller claim than the \
         one recorded"
    } else {
        "declines less of the script than it did, which is an improvement and leaves the record \
         stale — a number nobody updates is a number that stops meaning anything"
    };
    Err(Error::failed(format!(
        "the {gate} run at {size} skipped {found} steps and is recorded as skipping {expected}. It \
         {direction}. A step is skipped when the two windows have already laid the document out \
         differently; if that has genuinely changed, record it in `{here}` in the same commit as \
         whatever changed it."
    )))
}

#[cfg(test)]
mod tests {
    use super::{check, reported};

    /// A tally line in the shape the harness prints it.
    const LINE: &str = "hit_results_agree_with_a_cold_window ok  size=s2 compared=76 apart=19 \
                        points=29184 landed=25010 deepest=7 faults=0";

    #[test]
    fn the_count_is_read_out_of_the_tally() {
        assert_eq!(reported(LINE), Some(19));
        assert_eq!(reported("nothing here"), None);
    }

    #[test]
    fn a_run_that_matches_its_record_passes() {
        assert!(check("hits", "s2", 19, LINE, "here").is_ok());
    }

    #[test]
    fn skipping_more_is_a_smaller_claim_and_fails() {
        let error = check("hits", "s2", 10, LINE, "xtask/src/oracle/subject.rs")
            .expect_err("nineteen against a recorded ten");
        let message = error.to_string();
        assert!(message.contains("smaller claim"), "{message}");
        assert!(message.contains("xtask/src/oracle/subject.rs"), "{message}");
    }

    #[test]
    fn skipping_fewer_leaves_the_record_stale_and_also_fails() {
        // The half that is easy to leave out, and the half that rots: a gate quietly comparing more
        // than its record says is a gate whose record nobody has read since it was written.
        let error = check("hits", "s2", 30, LINE, "here").expect_err("nineteen against thirty");
        assert!(error.to_string().contains("stale"), "{error}");
    }

    #[test]
    fn a_run_with_no_tally_at_all_says_so() {
        let error = check("hits", "s2", 19, "", "here").expect_err("no tally");
        assert!(error.to_string().contains("printed no tally"), "{error}");
    }
}
