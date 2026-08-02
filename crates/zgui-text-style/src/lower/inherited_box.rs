//! The inherited-box group: the two properties that decide which way a paragraph runs.

use zgui_css::computed::style_structs;
use zgui_css::values::text;

use crate::style::paragraph::Direction;
use crate::style::writing::WritingMode;

/// `direction`.
pub fn direction(group: &style_structs::InheritedBox) -> Direction {
    match group.direction {
        text::Direction::Ltr => Direction::LeftToRight,
        text::Direction::Rtl => Direction::RightToLeft,
    }
}

/// `writing-mode`.
///
/// The two sideways keywords the CSS grammar also defines are built for another engine and are not
/// values this framework's parser accepts, so there is nothing to map them from.
pub fn writing_mode(group: &style_structs::InheritedBox) -> WritingMode {
    match group.writing_mode {
        text::WritingModeProperty::HorizontalTb => WritingMode::HorizontalTb,
        text::WritingModeProperty::VerticalRl => WritingMode::VerticalRl,
        text::WritingModeProperty::VerticalLr => WritingMode::VerticalLr,
    }
}
