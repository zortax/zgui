//! Which boxes need measuring before they can be laid out.
//!
//! `min-content`, `max-content` and `fit-content` have no representation in the layout algorithms'
//! own sizing model, so a box written with one of them is measured first and the answer substituted
//! for the keyword. Every other value goes straight through, which is why this asks the question
//! per box rather than measuring everything.

use zgui_css::ComputedStyle;
use zgui_css::values::size::{MaxSizeValue, SizeValue};

use crate::axis::Axis;
use crate::style::StyleRef;

/// Whether one size value needs the content measured before it means anything.
pub fn size_needs_measuring(value: &SizeValue) -> bool {
    matches!(
        value,
        SizeValue::MinContent
            | SizeValue::MaxContent
            | SizeValue::FitContent
            | SizeValue::FitContentFunction(_)
    )
}

/// The same question for `max-width` and `max-height`.
pub fn max_size_needs_measuring(value: &MaxSizeValue) -> bool {
    matches!(
        value,
        MaxSizeValue::MinContent
            | MaxSizeValue::MaxContent
            | MaxSizeValue::FitContent
            | MaxSizeValue::FitContentFunction(_)
    )
}

/// Which of a box's axes carry a keyword that needs the content measured.
///
/// This is the sole definition of "is a content-keyword box", and both the roster that decides
/// which boxes the pre-pass visits and the pre-pass itself answer the question by calling it. That
/// is not tidiness. Two spellings of this predicate that drift apart give a box the roster never
/// registers and the pre-pass would have measured: its keyword goes unanswered, reads as `auto`,
/// and the document lays out to a size its content never asked for — with every box in it
/// internally consistent and nothing anywhere reporting that a measurement was skipped.
pub fn axes_of(style: &ComputedStyle) -> [bool; 2] {
    let position = style.get_position();
    [
        size_needs_measuring(&position.width)
            || size_needs_measuring(&position.min_width)
            || max_size_needs_measuring(&position.max_width),
        size_needs_measuring(&position.height)
            || size_needs_measuring(&position.min_height)
            || max_size_needs_measuring(&position.max_height),
    ]
}

/// The same question asked of a box the layout algorithms are already looking at.
pub fn axes_needing_measurement(style: StyleRef<'_>) -> [bool; 2] {
    axes_of(style.style())
}

/// Whether the given axis needs measuring.
pub fn needs_measurement(style: StyleRef<'_>, axis: Axis) -> bool {
    let axes = axes_needing_measurement(style);
    match axis {
        Axis::Horizontal => axes[0],
        Axis::Vertical => axes[1],
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::values::length::{Length, LengthPercentage, NonNegative};
    use zgui_css::values::size::{MaxSizeValue, SizeValue};

    use super::{max_size_needs_measuring, size_needs_measuring};

    #[test]
    fn only_the_content_keywords_need_measuring() {
        assert!(size_needs_measuring(&SizeValue::MinContent));
        assert!(size_needs_measuring(&SizeValue::MaxContent));
        assert!(size_needs_measuring(&SizeValue::FitContent));
        assert!(!size_needs_measuring(&SizeValue::Auto));
        assert!(!size_needs_measuring(&SizeValue::Stretch));
        assert!(!size_needs_measuring(&SizeValue::LengthPercentage(
            NonNegative(LengthPercentage::new_length(Length::new(10.0)))
        )));
    }

    #[test]
    fn a_maximum_asks_the_same_question_of_its_own_value_set() {
        assert!(max_size_needs_measuring(&MaxSizeValue::MinContent));
        assert!(!max_size_needs_measuring(&MaxSizeValue::None));
    }
}
