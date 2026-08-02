//! The properties flex layout reads beyond the core set.

use taffy::prelude::TaffyAuto;
use taffy::{
    AlignContent, AlignItems, AlignSelf, Dimension, FlexDirection, FlexWrap, FlexboxContainerStyle,
    FlexboxItemStyle, JustifyContent, LengthPercentage, Size,
};
use zgui_css::values::flex::{FlexDirectionValue, FlexWrapValue};

use crate::style::StyleRef;
use crate::style::convert::{align, length};
use crate::style::gap::gap_of;

impl FlexboxContainerStyle for StyleRef<'_> {
    fn flex_direction(&self) -> FlexDirection {
        match self.position_group().flex_direction {
            FlexDirectionValue::Row => FlexDirection::Row,
            FlexDirectionValue::RowReverse => FlexDirection::RowReverse,
            FlexDirectionValue::Column => FlexDirection::Column,
            FlexDirectionValue::ColumnReverse => FlexDirection::ColumnReverse,
        }
    }

    fn flex_wrap(&self) -> FlexWrap {
        match self.position_group().flex_wrap {
            FlexWrapValue::Nowrap => FlexWrap::NoWrap,
            FlexWrapValue::Wrap => FlexWrap::Wrap,
            FlexWrapValue::WrapReverse => FlexWrap::WrapReverse,
        }
    }

    fn gap(&self) -> Size<LengthPercentage> {
        gap_of(*self)
    }

    fn align_content(&self) -> Option<AlignContent> {
        align::align_content(self.position_group().align_content.primary(), self.is_rtl())
    }

    fn align_items(&self) -> Option<AlignItems> {
        align::align_items(self.position_group().align_items.0, self.is_rtl())
    }

    fn justify_content(&self) -> Option<JustifyContent> {
        align::align_content(
            self.position_group().justify_content.primary(),
            self.is_rtl(),
        )
    }
}

impl FlexboxItemStyle for StyleRef<'_> {
    fn flex_basis(&self) -> Dimension {
        // `content` means "size from the content, ignoring `width`", which has no representation
        // here; `auto` is the nearest, and it defers to `width` instead of overriding it.
        let (scale, calc, measured) = (self.scale(), self.calc(), self.measured());
        match &self.position_group().flex_basis {
            zgui_css::values::size::FlexBasisValue::Size(size) => {
                length::size(size, scale, calc, measured.horizontal)
            }
            zgui_css::values::size::FlexBasisValue::Content => Dimension::AUTO,
        }
    }

    fn flex_grow(&self) -> f32 {
        self.position_group().flex_grow.0
    }

    fn flex_shrink(&self) -> f32 {
        self.position_group().flex_shrink.0
    }

    fn align_self(&self) -> Option<AlignSelf> {
        align::align_items(self.position_group().align_self.0, self.is_rtl())
    }
}
