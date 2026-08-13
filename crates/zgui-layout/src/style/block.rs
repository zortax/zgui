//! The properties block flow reads beyond the core set.

use taffy::{AlignContent, BlockContainerStyle, BlockItemStyle, Clear, Float, TextAlign};

use crate::node::kind::FormattingContext;
use crate::style::StyleRef;

impl BlockContainerStyle for StyleRef<'_> {
    fn text_align(&self) -> TextAlign {
        // Only the three legacy values reach block layout, and they align block-level *boxes*
        // rather than text. Real `text-align` is a property of the lines inside a box and is
        // consumed where lines are made.
        self.lowered().text_align
    }

    fn align_content(&self) -> Option<AlignContent> {
        self.lowered().align_content
    }
}

impl BlockItemStyle for StyleRef<'_> {
    fn is_table(&self) -> bool {
        self.node().fc == FormattingContext::Table
    }

    fn float(&self) -> Float {
        self.lowered().float
    }

    fn clear(&self) -> Clear {
        self.lowered().clear
    }
}
