//! Which line a parent aligns a whole inline formatting context by.
//!
//! Two different answers are needed and neither substitutes for the other. Something aligned
//! against the *first* baseline of a paragraph — a flex item in a baseline-aligned row — is aligned
//! by its first line; something in normal flow, which is what an `inline-block` is, is aligned by
//! its *last* line, so that a two-line box sits on the text around it by its bottom line rather
//! than lifting the whole line to reach its top one.

use crate::inline::lines::LineBox;

/// The baseline of the first line, measured down from the top of the context's content box.
///
/// Absent when the context has no lines at all, which is what an empty one has — and absent rather
/// than zero, because a parent with no baseline to align to falls back to the bottom margin edge
/// and a zero would align it to its top.
pub fn first(lines: &[LineBox]) -> Option<f32> {
    lines.first().map(LineBox::baseline)
}

/// The baseline of the last line, measured the same way.
pub fn last(lines: &[LineBox]) -> Option<f32> {
    lines.last().map(LineBox::baseline)
}

#[cfg(test)]
mod tests {
    use crate::inline::lines::LineBox;
    use crate::inline::strut::Extents;

    use super::{first, last};

    /// Two stacked lines, the first 20 tall and the second 30.
    fn lines() -> Vec<LineBox> {
        vec![
            LineBox {
                text: 0..4,
                top: 0.0,
                extents: Extents {
                    above: 16.0,
                    below: 4.0,
                },
                width: 40.0,
                offset: 0.0,
            },
            LineBox {
                text: 4..9,
                top: 20.0,
                extents: Extents {
                    above: 24.0,
                    below: 6.0,
                },
                width: 30.0,
                offset: 0.0,
            },
        ]
    }

    #[test]
    fn the_two_baselines_are_different_lines_and_not_the_same_number() {
        assert_eq!(first(&lines()), Some(16.0));
        assert_eq!(last(&lines()), Some(44.0));
        assert_eq!(first(&[]), None);
        assert_eq!(last(&[]), None);
    }
}
