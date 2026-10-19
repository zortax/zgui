//! One computed style in the incremental engine's vocabulary.
//!
//! The engine reads one plain [`cephal::Style`] per box. Most of it is a property of the cascade
//! result and the device, so the lowering builds that part once as a template. What the engine
//! keeps in the style but zgui decides per box — the outer display, whether the box is a table or
//! replaced, its natural ratio, a gutter layout reserved, a `min-*`/`max-*` keyword's measured
//! value — is a [`Variant`], and [`engine_style`] patches it into a copy of the template. The
//! store interns the result, so boxes that agree share one entry.

pub(crate) mod align;
pub(crate) mod grid;
pub(crate) mod length;

use cephal::style::{
    BoxSizing, Clear, Contain, Direction, Display, FlexDirection, FlexWrap, Float,
    LengthPercentageAuto, Overflow, Position, Style, TextAlign,
};
use cephal::{Point, Rect, Size};
use zgui_css::ComputedStyle;
use zgui_css::values::flex::{FlexDirectionValue, FlexWrapValue};
use zgui_css::values::size::{BoxSizingValue, ClearValue, FloatValue, PositionValue};
use zgui_css::values::text::TextAlignKeyword;

use crate::node::box_node::BoxNode;
use crate::node::kind::FormattingContext;
use crate::style::calc::InternCalc;
use crate::style::convert::overflow;
use crate::style::grid::idents::IdentTable;
use crate::style::lowered::LayoutStyle;
use crate::style::{DeviceStyle, MeasuredSizes};

/// What one box adds to its lowering's template.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Variant {
    /// The outer display the engine dispatches on.
    pub(crate) display: Display,
    /// Whether the box is laid out by the table rules.
    pub(crate) item_is_table: bool,
    /// Whether the box is replaced content.
    pub(crate) item_is_replaced: bool,
    /// The natural proportions of the box's content, if it has any.
    pub(crate) natural_ratio: Option<f32>,
    /// Which axes layout has decided reserve a scrollbar gutter.
    pub(crate) gutter: (bool, bool),
    /// What the intrinsic pre-pass measured for the box.
    pub(crate) measured: MeasuredSizes,
}

/// The part of the engine style that follows from the cascade result and the device alone.
pub(crate) fn template(
    style: &ComputedStyle,
    device: DeviceStyle,
    calc: &mut impl InternCalc,
    idents: &mut IdentTable,
) -> Style {
    let scale = device.scale;
    let box_ = style.get_box();
    let position = style.get_position();
    let margin = style.get_margin();
    let padding = style.get_padding();
    let border = style.get_border();
    let rtl = style.get_inherited_box().direction == zgui_css::values::text::Direction::Rtl;
    let inset = if box_.position == PositionValue::Static {
        Rect::splat(LengthPercentageAuto::AUTO)
    } else {
        Rect {
            left: length::inset(&position.left, scale, calc),
            right: length::inset(&position.right, scale, calc),
            top: length::inset(&position.top, scale, calc),
            bottom: length::inset(&position.bottom, scale, calc),
        }
    };
    Style {
        display: Display::Block,
        box_sizing: match position.box_sizing {
            BoxSizingValue::ContentBox => BoxSizing::ContentBox,
            BoxSizingValue::BorderBox => BoxSizing::BorderBox,
        },
        direction: if rtl { Direction::Rtl } else { Direction::Ltr },
        // Static boxes ignore their insets; sticky ones are laid out as static ones.
        position: match box_.position {
            PositionValue::Static => Position::Static,
            PositionValue::Relative => Position::Relative,
            PositionValue::Sticky => Position::Sticky,
            PositionValue::Absolute => Position::Absolute,
            PositionValue::Fixed => Position::Fixed,
        },
        overflow: Point {
            x: overflow::overflow(box_.overflow_x),
            y: overflow::overflow(box_.overflow_y),
        },
        contain: Contain::NONE,
        float: match box_.float {
            FloatValue::None => Float::None,
            FloatValue::Left => Float::Left,
            FloatValue::Right => Float::Right,
            FloatValue::InlineStart => flow(rtl, Float::Left, Float::Right),
            FloatValue::InlineEnd => flow(rtl, Float::Right, Float::Left),
        },
        clear: match box_.clear {
            ClearValue::None => Clear::None,
            ClearValue::Left => Clear::Left,
            ClearValue::Right => Clear::Right,
            ClearValue::Both => Clear::Both,
            ClearValue::InlineStart => flow(rtl, Clear::Left, Clear::Right),
            ClearValue::InlineEnd => flow(rtl, Clear::Right, Clear::Left),
        },
        text_align: match style.get_inherited_text().text_align {
            TextAlignKeyword::MozLeft => TextAlign::LegacyLeft,
            TextAlignKeyword::MozRight => TextAlign::LegacyRight,
            TextAlignKeyword::MozCenter => TextAlign::LegacyCenter,
            _ => TextAlign::Auto,
        },
        item_is_table: false,
        item_is_replaced: false,
        scrollbar_width: device.scrollbar_width,
        aspect_ratio: None,
        size: Size {
            width: length::size(&position.width, scale, calc),
            height: length::size(&position.height, scale, calc),
        },
        min_size: Size {
            width: length::min_size(&position.min_width, scale, calc),
            height: length::min_size(&position.min_height, scale, calc),
        },
        max_size: Size {
            width: length::max_size(&position.max_width, scale, calc),
            height: length::max_size(&position.max_height, scale, calc),
        },
        inset,
        margin: Rect {
            left: length::margin(&margin.margin_left, scale, calc),
            right: length::margin(&margin.margin_right, scale, calc),
            top: length::margin(&margin.margin_top, scale, calc),
            bottom: length::margin(&margin.margin_bottom, scale, calc),
        },
        padding: Rect {
            left: length::padding(&padding.padding_left, scale, calc),
            right: length::padding(&padding.padding_right, scale, calc),
            top: length::padding(&padding.padding_top, scale, calc),
            bottom: length::padding(&padding.padding_bottom, scale, calc),
        },
        border: Rect {
            left: length::border_side(&border.border_left_width, border.border_left_style, scale),
            right: length::border_side(
                &border.border_right_width,
                border.border_right_style,
                scale,
            ),
            top: length::border_side(&border.border_top_width, border.border_top_style, scale),
            bottom: length::border_side(
                &border.border_bottom_width,
                border.border_bottom_style,
                scale,
            ),
        },
        gap: Size {
            width: length::gap(&position.column_gap, scale, calc),
            height: length::gap(&position.row_gap, scale, calc),
        },
        align_items: align::items(position.align_items.0, rtl),
        align_self: align::items(position.align_self.0, rtl),
        justify_items: align::justify_items((position.justify_items.computed.0).0, rtl),
        justify_self: align::items(position.justify_self.0, rtl),
        align_content: align::content(position.align_content.primary(), rtl),
        justify_content: align::content(position.justify_content.primary(), rtl),
        flex_direction: match position.flex_direction {
            FlexDirectionValue::Row => FlexDirection::Row,
            FlexDirectionValue::RowReverse => FlexDirection::RowReverse,
            FlexDirectionValue::Column => FlexDirection::Column,
            FlexDirectionValue::ColumnReverse => FlexDirection::ColumnReverse,
        },
        flex_wrap: match position.flex_wrap {
            FlexWrapValue::Nowrap => FlexWrap::NoWrap,
            FlexWrapValue::Wrap => FlexWrap::Wrap,
            FlexWrapValue::WrapReverse => FlexWrap::WrapReverse,
        },
        flex_line_count: 1,
        flex_basis: length::flex_basis(&position.flex_basis, scale, calc),
        flex_grow: position.flex_grow.0,
        flex_shrink: position.flex_shrink.0,
        grid_row: grid::line(&position.grid_row_start, &position.grid_row_end, idents),
        grid_column: grid::line(
            &position.grid_column_start,
            &position.grid_column_end,
            idents,
        ),
        grid: grid::container(position, scale, idents),
    }
}

fn flow<T>(rtl: bool, ltr: T, rtl_answer: T) -> T {
    if rtl { rtl_answer } else { ltr }
}

/// The outer display the engine dispatches one box on.
///
/// Mirrors the formatting-context dispatch: a box that holds lines is block-level when it sits
/// in block flow, an atomic inline runs the algorithm its inner display names, and a replaced or
/// custom box is a leaf that establishes its own context.
pub(crate) fn display(node: &BoxNode, lowered: &LayoutStyle) -> Display {
    let collapsed = lowered.collapses_as_flex_item && node.parent_fc == FormattingContext::Flex;
    if collapsed {
        return Display::None;
    }
    match node.fc {
        FormattingContext::None => Display::None,
        FormattingContext::Flex => Display::Flex,
        FormattingContext::Grid => Display::Grid,
        FormattingContext::Block | FormattingContext::Table | FormattingContext::MultiColumn => {
            Display::Block
        }
        FormattingContext::Inline => {
            if node.block_level {
                Display::Block
            } else {
                Display::FlowRoot
            }
        }
        FormattingContext::Atomic => {
            match crate::style::convert::display::atomic_inner(node.style.get_box().display) {
                FormattingContext::Flex => Display::Flex,
                FormattingContext::Grid => Display::Grid,
                _ => Display::FlowRoot,
            }
        }
        FormattingContext::Replaced | FormattingContext::Custom => Display::FlowRoot,
    }
}

/// The variant one box adds to its lowering.
pub(crate) fn variant(
    node: &BoxNode,
    lowered: &LayoutStyle,
    natural_ratio: Option<f32>,
    gutter: (bool, bool),
    measured: MeasuredSizes,
) -> Variant {
    Variant {
        display: display(node, lowered),
        item_is_table: node.fc == FormattingContext::Table,
        item_is_replaced: node.fc == FormattingContext::Replaced,
        natural_ratio,
        gutter,
        measured,
    }
}

/// The template with one box's variant patched in.
pub(crate) fn engine_style(lowered: &LayoutStyle, variant: &Variant) -> Style {
    let mut style = lowered.template.clone();
    style.display = variant.display;
    style.item_is_table = variant.item_is_table;
    style.item_is_replaced = variant.item_is_replaced;
    style.aspect_ratio = lowered.aspect_ratio(variant.natural_ratio);
    // An axis layout has decided reserves a gutter is `scroll` to the engine whatever the style
    // says, because reserving the space is what that value means to it.
    if variant.gutter.0 {
        style.overflow.x = Overflow::Scroll;
    }
    if variant.gutter.1 {
        style.overflow.y = Overflow::Scroll;
    }
    let inset = lowered.intrinsic_inset;
    let measured = MeasuredSizes {
        horizontal: variant
            .measured
            .horizontal
            .map(|sizes| sizes.less(inset.width)),
        vertical: variant
            .measured
            .vertical
            .map(|sizes| sizes.less(inset.height)),
    };
    let [min_width, min_height, max_width, max_height] = lowered.min_max_with(measured);
    let substitute = |slot: &mut LengthPercentageAuto, value: Option<f32>| {
        if let Some(value) = value {
            *slot = LengthPercentageAuto::length(value);
        }
    };
    substitute(&mut style.min_size.width, min_width);
    substitute(&mut style.min_size.height, min_height);
    substitute(&mut style.max_size.width, max_width);
    substitute(&mut style.max_size.height, max_height);
    style
}

#[cfg(test)]
mod tests {
    use cephal::style::{Dimension, Display, LengthPercentageAuto, Overflow};
    use zgui_css::StyleDraft;
    use zgui_css::values::length::{Length, LengthPercentage as CssLp, NonNegative};
    use zgui_css::values::size::{MaxSizeValue, OverflowValue, SizeValue};

    use crate::node::box_node::BoxNode;
    use crate::node::kind::FormattingContext;
    use crate::style::calc::CalcTable;
    use crate::style::convert::length::IntrinsicSizes;
    use crate::style::grid::idents::IdentTable;
    use crate::style::lowered::LayoutStyle;
    use crate::style::{DeviceStyle, MeasuredSizes};

    use super::{Variant, engine_style};

    fn lower_with(device: DeviceStyle, mutate: impl FnOnce(&mut StyleDraft)) -> LayoutStyle {
        let mut draft = StyleDraft::initial();
        mutate(&mut draft);
        let style = draft.build();
        let mut calc = CalcTable::default();
        calc.set_scale(device.scale);
        let mut idents = IdentTable::default();
        LayoutStyle::lower(&style, device, &mut calc, &mut idents)
    }

    fn px(value: f32) -> CssLp {
        CssLp::new_length(Length::new(value))
    }

    #[test]
    fn sizing_keywords_travel_as_themselves_and_min_max_keywords_defer() {
        let lowered = lower_with(DeviceStyle::default(), |draft| {
            let group = draft.position_group();
            group.width = SizeValue::MinContent;
            group.height = SizeValue::Stretch;
            group.min_width = SizeValue::MaxContent;
            group.max_height = MaxSizeValue::FitContent;
        });
        let template = &lowered.template;
        assert_eq!(template.size.width, Dimension::MIN_CONTENT);
        assert_eq!(template.size.height, Dimension::STRETCH);
        assert_eq!(template.min_size.width, LengthPercentageAuto::AUTO);
        assert_eq!(template.max_size.height, LengthPercentageAuto::AUTO);
        assert!(template.grid.is_none(), "an initial grid is no payload");
    }

    #[test]
    fn absolute_lengths_reach_the_template_in_device_pixels() {
        let device = DeviceStyle {
            scale: 2.0,
            scrollbar_width: 12.0,
        };
        let lowered = lower_with(device, |draft| {
            draft.position_group().width = SizeValue::LengthPercentage(NonNegative(px(10.0)));
            draft.padding().padding_left = NonNegative(px(3.0));
        });
        assert_eq!(lowered.template.size.width, Dimension::length(20.0));
        assert_eq!(lowered.template.padding.left.value(), 6.0);
        assert_eq!(lowered.template.scrollbar_width, 12.0);
    }

    #[test]
    fn a_variant_patches_the_display_the_gutter_and_the_measured_keywords() {
        let lowered = lower_with(DeviceStyle::default(), |draft| {
            draft.position_group().min_width = SizeValue::MaxContent;
            draft.box_group().overflow_y = OverflowValue::Auto;
        });
        assert_eq!(lowered.template.overflow.y, Overflow::Hidden);
        let variant = Variant {
            display: Display::Flex,
            item_is_table: false,
            item_is_replaced: false,
            natural_ratio: Some(2.0),
            gutter: (false, true),
            measured: MeasuredSizes {
                horizontal: Some(IntrinsicSizes {
                    min: 30.0,
                    max: 90.0,
                }),
                vertical: None,
            },
        };
        let style = engine_style(&lowered, &variant);
        assert_eq!(style.display, Display::Flex);
        assert_eq!(style.overflow.y, Overflow::Scroll);
        assert_eq!(style.overflow.x, Overflow::Visible);
        assert_eq!(style.min_size.width, LengthPercentageAuto::length(90.0));
        assert_eq!(
            style.aspect_ratio,
            Some(2.0),
            "no written ratio defers to the content"
        );
        let unmeasured = engine_style(&lowered, &Variant::default());
        assert_eq!(unmeasured.min_size.width, LengthPercentageAuto::AUTO);
    }

    #[test]
    fn the_display_follows_the_formatting_context_and_the_block_level_flag() {
        let lowered = lower_with(DeviceStyle::default(), |_| {});
        let style = StyleDraft::initial().build();
        let mut node = BoxNode::new(
            style,
            crate::node::kind::BoxKind::Element,
            FormattingContext::Inline,
        );
        node.block_level = true;
        assert_eq!(super::display(&node, &lowered), Display::Block);
        node.block_level = false;
        assert_eq!(super::display(&node, &lowered), Display::FlowRoot);
        node.fc = FormattingContext::Grid;
        assert_eq!(super::display(&node, &lowered), Display::Grid);
        node.fc = FormattingContext::None;
        assert_eq!(super::display(&node, &lowered), Display::None);
    }
}
