//! The properties flex layout reads beyond the core set.

use taffy::{
    AlignContent, AlignItems, AlignSelf, Dimension, FlexDirection, FlexWrap, FlexboxContainerStyle,
    FlexboxItemStyle, JustifyContent, LengthPercentage, Size,
};

use crate::style::StyleRef;

impl FlexboxContainerStyle for StyleRef<'_> {
    fn flex_direction(&self) -> FlexDirection {
        self.lowered().flex_direction
    }

    fn flex_wrap(&self) -> FlexWrap {
        self.lowered().flex_wrap
    }

    fn gap(&self) -> Size<LengthPercentage> {
        self.lowered().gap
    }

    fn align_content(&self) -> Option<AlignContent> {
        self.lowered().align_content
    }

    fn align_items(&self) -> Option<AlignItems> {
        self.lowered().align_items
    }

    fn justify_content(&self) -> Option<JustifyContent> {
        self.lowered().justify_content
    }
}

impl FlexboxItemStyle for StyleRef<'_> {
    fn flex_basis(&self) -> Dimension {
        self.lowered().flex_basis_with(self.measured())
    }

    fn flex_grow(&self) -> f32 {
        self.lowered().flex_grow
    }

    fn flex_shrink(&self) -> f32 {
        self.lowered().flex_shrink
    }

    fn align_self(&self) -> Option<AlignSelf> {
        self.lowered().align_self
    }
}
