//! What to print when two blocks of text differ.
//!
//! A golden failure that prints two whole trees is a failure nobody reads. What is wanted is the
//! first line that differs, with a little context, and the counts either side.

/// A line-by-line report of the first difference between `expected` and `actual`.
///
/// Returns `None` when they are identical.
pub fn first_difference(expected: &str, actual: &str) -> Option<String> {
    if expected == actual {
        return None;
    }
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let at = expected_lines
        .iter()
        .zip(&actual_lines)
        .position(|(left, right)| left != right)
        .unwrap_or(expected_lines.len().min(actual_lines.len()));

    let mut report = format!(
        "goldens differ at line {} ({} expected lines, {} actual)\n",
        at + 1,
        expected_lines.len(),
        actual_lines.len()
    );
    for index in at.saturating_sub(2)..at {
        if let Some(line) = expected_lines.get(index) {
            report.push_str(&format!("  {index:>4} | {line}\n"));
        }
    }
    report.push_str(&format!(
        "- {:>4} | {}\n",
        at + 1,
        expected_lines.get(at).copied().unwrap_or("<end of file>")
    ));
    report.push_str(&format!(
        "+ {:>4} | {}\n",
        at + 1,
        actual_lines.get(at).copied().unwrap_or("<end of file>")
    ));
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::first_difference;

    #[test]
    fn identical_text_has_no_difference() {
        assert!(first_difference("a\nb\n", "a\nb\n").is_none());
    }

    #[test]
    fn the_report_names_the_first_differing_line_and_both_sides() {
        let report = first_difference("a\nb\nc\n", "a\nB\nc\n").expect("they differ");
        assert!(report.contains("line 2"));
        assert!(report.contains("- "));
        assert!(report.contains("+ "));
        assert!(report.contains('B'));
    }

    #[test]
    fn a_truncated_side_is_reported_as_the_end_of_the_file() {
        let report = first_difference("a\nb\n", "a\n").expect("they differ");
        assert!(report.contains("<end of file>"));
        assert!(report.contains("2 expected lines, 1 actual"));
    }
}
