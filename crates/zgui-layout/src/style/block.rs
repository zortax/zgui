//! The properties block flow reads beyond the core set.

use taffy::{AlignContent, BlockContainerStyle, BlockItemStyle, Clear, Float, TextAlign};
use zgui_css::values::size::{ClearValue, FloatValue};
use zgui_css::values::text::TextAlignKeyword;

use crate::node::kind::FormattingContext;
use crate::style::StyleRef;
use crate::style::convert::align;

impl BlockContainerStyle for StyleRef<'_> {
    fn text_align(&self) -> TextAlign {
        // Only the three legacy values reach block layout, and they align block-level *boxes*
        // rather than text. Real `text-align` is a property of the lines inside a box and is
        // consumed where lines are made.
        match self.style().get_inherited_text().text_align {
            TextAlignKeyword::MozLeft => TextAlign::LegacyLeft,
            TextAlignKeyword::MozRight => TextAlign::LegacyRight,
            TextAlignKeyword::MozCenter => TextAlign::LegacyCenter,
            _ => TextAlign::Auto,
        }
    }

    fn align_content(&self) -> Option<AlignContent> {
        align::align_content(self.position_group().align_content.primary(), self.is_rtl())
    }
}

impl BlockItemStyle for StyleRef<'_> {
    fn is_table(&self) -> bool {
        self.node().fc == FormattingContext::Table
    }

    fn float(&self) -> Float {
        // The two flow-relative keywords resolve against the writing direction, which is the only
        // thing separating them from the physical pair.
        match self.box_().float {
            FloatValue::None => Float::None,
            FloatValue::Left => Float::Left,
            FloatValue::Right => Float::Right,
            FloatValue::InlineStart => self.flow_relative(Float::Left, Float::Right),
            FloatValue::InlineEnd => self.flow_relative(Float::Right, Float::Left),
        }
    }

    fn clear(&self) -> Clear {
        match self.box_().clear {
            ClearValue::None => Clear::None,
            ClearValue::Left => Clear::Left,
            ClearValue::Right => Clear::Right,
            ClearValue::Both => Clear::Both,
            ClearValue::InlineStart => self.flow_relative(Clear::Left, Clear::Right),
            ClearValue::InlineEnd => self.flow_relative(Clear::Right, Clear::Left),
        }
    }
}

impl StyleRef<'_> {
    /// Picks the left-to-right answer or the right-to-left one.
    fn flow_relative<T>(&self, ltr: T, rtl: T) -> T {
        if self.is_rtl() { rtl } else { ltr }
    }
}
