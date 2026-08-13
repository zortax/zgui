//! One computed style, lowered once into the form the layout algorithms read.
//!
//! The style traits used to convert CSS values on every accessor call, which put the same
//! conversions inside every hot layout loop — measured at roughly a fifth of the self time of a
//! keystroke, a scroll and a resize. A [`LayoutStyle`] is that conversion done once per distinct
//! cascade result and device, held by the store's style table, and read by [`StyleRef`] as plain
//! fields.
//!
//! What cannot be lowered stays out. A content keyword resolves against a *box's* measured
//! intrinsic sizes, so the packed value holds `auto` and a [`Keywords`] slot says which
//! measurement to substitute at read time. An `auto` aspect ratio defers to a box's natural
//! ratio the same way. Grid track templates stay on the computed style and are walked by the
//! existing lazy iterators, because they allocate nothing per call and their conversion is not
//! in any measured hot path.
//!
//! [`StyleRef`]: crate::style::StyleRef

use taffy::prelude::TaffyAuto;
use taffy::{
    AlignContent, AlignItems, AlignSelf, BoxSizing, Clear, Dimension, Direction, FlexDirection,
    FlexWrap, Float, GridAutoFlow, GridPlacement, JustifyContent, LengthPercentage,
    LengthPercentageAuto, Line, Overflow, Point, Position, Rect, Size, TextAlign,
};
use zgui_css::ComputedStyle;
use zgui_css::values::flex::{FlexDirectionValue, FlexWrapValue};
use zgui_css::values::grid::GridAutoFlowValue;
use zgui_css::values::size::{
    BoxSizingValue, ClearValue, FlexBasisValue, FloatValue, MaxSizeValue, PositionValue, SizeValue,
    VisibilityValue,
};
use zgui_css::values::text::TextAlignKeyword;
use zgui_interned::Ident;

use crate::style::calc::InternCalc;
use crate::style::convert::length::IntrinsicSizes;
use crate::style::convert::{align, aspect, length, overflow};
use crate::style::gap::gap_value;
use crate::style::{DeviceStyle, MeasuredSizes};

/// Which per-box measurement a size slot substitutes, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keyword {
    /// The packed value is final.
    None = 0,
    /// Substitute the content's minimum size.
    Min = 1,
    /// Substitute the content's maximum size. `fit-content` lands here too, exactly as the
    /// per-call conversion resolved it.
    Max = 2,
}

/// One two-bit keyword slot per substitutable size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Keywords(u16);

/// The substitutable size slots, two bits each.
#[derive(Clone, Copy, Debug)]
enum Slot {
    Width = 0,
    Height = 1,
    MinWidth = 2,
    MinHeight = 3,
    MaxWidth = 4,
    MaxHeight = 5,
    FlexBasis = 6,
}

impl Keywords {
    fn set(&mut self, slot: Slot, keyword: Keyword) {
        self.0 |= (keyword as u16) << ((slot as u16) * 2);
    }

    fn get(self, slot: Slot) -> Keyword {
        match (self.0 >> ((slot as u16) * 2)) & 0b11 {
            1 => Keyword::Min,
            2 => Keyword::Max,
            _ => Keyword::None,
        }
    }

    /// Whether no slot substitutes anything, which is the common box.
    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// One computed style in the layout algorithms' vocabulary, conversion already paid.
#[derive(Clone, Debug)]
pub(crate) struct LayoutStyle {
    /// `top`/`right`/`bottom`/`left`, already `auto` throughout for a static box.
    pub(crate) inset: Rect<LengthPercentageAuto>,
    /// `width`/`height`, `auto` where a keyword slot substitutes.
    pub(crate) size: Size<Dimension>,
    /// `min-width`/`min-height`.
    pub(crate) min_size: Size<Dimension>,
    /// `max-width`/`max-height`.
    pub(crate) max_size: Size<Dimension>,
    /// The four margins.
    pub(crate) margin: Rect<LengthPercentageAuto>,
    /// The four paddings.
    pub(crate) padding: Rect<LengthPercentage>,
    /// The four border widths, `none`/`hidden` sides already zero.
    pub(crate) border: Rect<LengthPercentage>,
    /// How much of an intrinsic measurement is this style's own padding and border, per axis.
    ///
    /// Zero under `border-box`. Percentage and `calc()` components contribute nothing, exactly as
    /// resolving them against no basis contributed nothing per call.
    pub(crate) intrinsic_inset: Size<f32>,
    /// The written aspect ratio, degenerate ratios already discarded.
    aspect_explicit: Option<f32>,
    /// `row-gap` and `column-gap`, the column gap on the inline axis.
    pub(crate) gap: Size<LengthPercentage>,
    /// `flex-basis`, `content` already `auto`.
    pub(crate) flex_basis: Dimension,
    /// `flex-grow`.
    pub(crate) flex_grow: f32,
    /// `flex-shrink`.
    pub(crate) flex_shrink: f32,
    /// The six alignment properties, writing direction already applied.
    pub(crate) align_content: Option<AlignContent>,
    pub(crate) justify_content: Option<JustifyContent>,
    pub(crate) align_items: Option<AlignItems>,
    pub(crate) justify_items: Option<AlignItems>,
    pub(crate) align_self: Option<AlignSelf>,
    pub(crate) justify_self: Option<AlignSelf>,
    /// `grid-row` placement.
    pub(crate) grid_row: Line<GridPlacement<Ident>>,
    /// `grid-column` placement.
    pub(crate) grid_column: Line<GridPlacement<Ident>>,
    /// Which size slots substitute a measurement at read time.
    pub(crate) keywords: Keywords,
    /// `box-sizing`.
    pub(crate) box_sizing: BoxSizing,
    /// The writing direction.
    pub(crate) direction: Direction,
    /// The position class the algorithms distinguish: in flow or absolutely placed.
    pub(crate) position: Position,
    /// Whether the box is `position: fixed`, told apart from absolute for overflow only.
    pub(crate) fixed: bool,
    /// `overflow` per axis before layout decides a gutter; a reserved axis reads as `Scroll`.
    pub(crate) overflow: Point<Overflow>,
    /// The three legacy block-alignment values, or `Auto`.
    pub(crate) text_align: TextAlign,
    /// `float`, flow-relative keywords already resolved.
    pub(crate) float: Float,
    /// `clear`, flow-relative keywords already resolved.
    pub(crate) clear: Clear,
    /// `flex-direction`.
    pub(crate) flex_direction: FlexDirection,
    /// `flex-wrap`.
    pub(crate) flex_wrap: FlexWrap,
    /// `grid-auto-flow`.
    pub(crate) grid_auto_flow: GridAutoFlow,
    /// Whether `aspect-ratio` prefers the content's natural proportions.
    aspect_auto: bool,
    /// Whether `visibility: collapse` removes this box when its parent is a flex container.
    pub(crate) collapses_as_flex_item: bool,
}

impl LayoutStyle {
    /// Lowers one computed style for one device.
    ///
    /// Every `calc()` met on the way is interned into `calc`, and the caller owns the identifiers
    /// the interner issued.
    pub(crate) fn lower(
        style: &ComputedStyle,
        device: DeviceStyle,
        calc: &mut impl InternCalc,
    ) -> Self {
        let scale = device.scale;
        let box_ = style.get_box();
        let position_group = style.get_position();
        let margin_group = style.get_margin();
        let padding_group = style.get_padding();
        let border_group = style.get_border();
        let rtl = style.get_inherited_box().direction == zgui_css::values::text::Direction::Rtl;

        let mut keywords = Keywords::default();
        keywords.set(Slot::Width, keyword_of_size(&position_group.width));
        keywords.set(Slot::Height, keyword_of_size(&position_group.height));
        keywords.set(Slot::MinWidth, keyword_of_size(&position_group.min_width));
        keywords.set(Slot::MinHeight, keyword_of_size(&position_group.min_height));
        keywords.set(Slot::MaxWidth, keyword_of_max(&position_group.max_width));
        keywords.set(Slot::MaxHeight, keyword_of_max(&position_group.max_height));
        if let FlexBasisValue::Size(size) = &position_group.flex_basis {
            keywords.set(Slot::FlexBasis, keyword_of_size(size));
        }

        let inset = if box_.position == PositionValue::Static {
            Rect::auto()
        } else {
            Rect {
                left: length::inset(&position_group.left, scale, calc),
                right: length::inset(&position_group.right, scale, calc),
                top: length::inset(&position_group.top, scale, calc),
                bottom: length::inset(&position_group.bottom, scale, calc),
            }
        };
        let padding = Rect {
            left: length::padding(&padding_group.padding_left, scale, calc),
            right: length::padding(&padding_group.padding_right, scale, calc),
            top: length::padding(&padding_group.padding_top, scale, calc),
            bottom: length::padding(&padding_group.padding_bottom, scale, calc),
        };
        let border = Rect {
            left: length::border_side(
                &border_group.border_left_width,
                border_group.border_left_style,
                scale,
            ),
            right: length::border_side(
                &border_group.border_right_width,
                border_group.border_right_style,
                scale,
            ),
            top: length::border_side(
                &border_group.border_top_width,
                border_group.border_top_style,
                scale,
            ),
            bottom: length::border_side(
                &border_group.border_bottom_width,
                border_group.border_bottom_style,
                scale,
            ),
        };
        let intrinsic_inset = if position_group.box_sizing == BoxSizingValue::ContentBox {
            lengths_of(&padding) + lengths_of(&border)
        } else {
            Size {
                width: 0.0,
                height: 0.0,
            }
        };

        let (explicit, auto) = aspect::split(&position_group.aspect_ratio);

        Self {
            inset,
            size: Size {
                width: length::size(&position_group.width, scale, calc, None),
                height: length::size(&position_group.height, scale, calc, None),
            },
            min_size: Size {
                width: length::size(&position_group.min_width, scale, calc, None),
                height: length::size(&position_group.min_height, scale, calc, None),
            },
            max_size: Size {
                width: length::max_size(&position_group.max_width, scale, calc, None),
                height: length::max_size(&position_group.max_height, scale, calc, None),
            },
            margin: Rect {
                left: length::margin(&margin_group.margin_left, scale, calc),
                right: length::margin(&margin_group.margin_right, scale, calc),
                top: length::margin(&margin_group.margin_top, scale, calc),
                bottom: length::margin(&margin_group.margin_bottom, scale, calc),
            },
            padding,
            border,
            intrinsic_inset,
            aspect_explicit: explicit,
            gap: Size {
                width: gap_value(&position_group.column_gap, scale, calc),
                height: gap_value(&position_group.row_gap, scale, calc),
            },
            flex_basis: match &position_group.flex_basis {
                FlexBasisValue::Size(size) => length::size(size, scale, calc, None),
                FlexBasisValue::Content => Dimension::AUTO,
            },
            flex_grow: position_group.flex_grow.0,
            flex_shrink: position_group.flex_shrink.0,
            align_content: align::align_content(position_group.align_content.primary(), rtl),
            justify_content: align::align_content(position_group.justify_content.primary(), rtl),
            align_items: align::align_items(position_group.align_items.0, rtl),
            justify_items: align::justify_items((position_group.justify_items.computed.0).0, rtl),
            align_self: align::align_items(position_group.align_self.0, rtl),
            justify_self: align::align_items(position_group.justify_self.0, rtl),
            grid_row: crate::style::grid::placement::line(
                &position_group.grid_row_start,
                &position_group.grid_row_end,
            ),
            grid_column: crate::style::grid::placement::line(
                &position_group.grid_column_start,
                &position_group.grid_column_end,
            ),
            keywords,
            box_sizing: match position_group.box_sizing {
                BoxSizingValue::ContentBox => BoxSizing::ContentBox,
                BoxSizingValue::BorderBox => BoxSizing::BorderBox,
            },
            direction: if rtl { Direction::Rtl } else { Direction::Ltr },
            position: match box_.position {
                PositionValue::Static | PositionValue::Relative | PositionValue::Sticky => {
                    Position::Relative
                }
                PositionValue::Absolute | PositionValue::Fixed => Position::Absolute,
            },
            fixed: box_.position == PositionValue::Fixed,
            overflow: Point {
                x: overflow::overflow(box_.overflow_x),
                y: overflow::overflow(box_.overflow_y),
            },
            text_align: match style.get_inherited_text().text_align {
                TextAlignKeyword::MozLeft => TextAlign::LegacyLeft,
                TextAlignKeyword::MozRight => TextAlign::LegacyRight,
                TextAlignKeyword::MozCenter => TextAlign::LegacyCenter,
                _ => TextAlign::Auto,
            },
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
            flex_direction: match position_group.flex_direction {
                FlexDirectionValue::Row => FlexDirection::Row,
                FlexDirectionValue::RowReverse => FlexDirection::RowReverse,
                FlexDirectionValue::Column => FlexDirection::Column,
                FlexDirectionValue::ColumnReverse => FlexDirection::ColumnReverse,
            },
            flex_wrap: match position_group.flex_wrap {
                FlexWrapValue::Nowrap => FlexWrap::NoWrap,
                FlexWrapValue::Wrap => FlexWrap::Wrap,
                FlexWrapValue::WrapReverse => FlexWrap::WrapReverse,
            },
            grid_auto_flow: {
                let raw = position_group.grid_auto_flow;
                let dense = raw.contains(GridAutoFlowValue::DENSE);
                match (raw.contains(GridAutoFlowValue::COLUMN), dense) {
                    (false, false) => GridAutoFlow::Row,
                    (false, true) => GridAutoFlow::RowDense,
                    (true, false) => GridAutoFlow::Column,
                    (true, true) => GridAutoFlow::ColumnDense,
                }
            },
            aspect_auto: auto,
            collapses_as_flex_item: style.get_inherited_box().visibility
                == VisibilityValue::Collapse
                && box_.display.outside() != zgui_css::values::size::DisplayOutside::None,
        }
    }

    /// `width`/`height` with any keyword substituted from `measured`.
    ///
    /// `measured` is stated the way a size in the style is stated — the caller has already taken
    /// [`LayoutStyle::intrinsic_inset`] off the raw measurement.
    pub(crate) fn size_with(&self, measured: MeasuredSizes) -> Size<Dimension> {
        if self.keywords.is_empty() {
            return self.size;
        }
        Size {
            width: substituted(
                self.size.width,
                self.keywords.get(Slot::Width),
                measured.horizontal,
            ),
            height: substituted(
                self.size.height,
                self.keywords.get(Slot::Height),
                measured.vertical,
            ),
        }
    }

    /// `min-width`/`min-height` with any keyword substituted from `measured`.
    pub(crate) fn min_size_with(&self, measured: MeasuredSizes) -> Size<Dimension> {
        if self.keywords.is_empty() {
            return self.min_size;
        }
        Size {
            width: substituted(
                self.min_size.width,
                self.keywords.get(Slot::MinWidth),
                measured.horizontal,
            ),
            height: substituted(
                self.min_size.height,
                self.keywords.get(Slot::MinHeight),
                measured.vertical,
            ),
        }
    }

    /// `max-width`/`max-height` with any keyword substituted from `measured`.
    pub(crate) fn max_size_with(&self, measured: MeasuredSizes) -> Size<Dimension> {
        if self.keywords.is_empty() {
            return self.max_size;
        }
        Size {
            width: substituted(
                self.max_size.width,
                self.keywords.get(Slot::MaxWidth),
                measured.horizontal,
            ),
            height: substituted(
                self.max_size.height,
                self.keywords.get(Slot::MaxHeight),
                measured.vertical,
            ),
        }
    }

    /// `flex-basis` with any keyword substituted from `measured`, which reads the inline axis.
    pub(crate) fn flex_basis_with(&self, measured: MeasuredSizes) -> Dimension {
        if self.keywords.is_empty() {
            return self.flex_basis;
        }
        substituted(
            self.flex_basis,
            self.keywords.get(Slot::FlexBasis),
            measured.horizontal,
        )
    }

    /// The ratio of width to height a box should keep, given its content's natural proportions.
    pub(crate) fn aspect_ratio(&self, natural: Option<f32>) -> Option<f32> {
        if self.aspect_auto {
            natural.or(self.aspect_explicit)
        } else {
            self.aspect_explicit.or(natural)
        }
    }
}

/// One size slot's value, the keyword substituted where one was written.
fn substituted(packed: Dimension, keyword: Keyword, measured: Option<IntrinsicSizes>) -> Dimension {
    match keyword {
        Keyword::None => packed,
        Keyword::Min => measured.map_or(Dimension::AUTO, |sizes| Dimension::length(sizes.min)),
        Keyword::Max => measured.map_or(Dimension::AUTO, |sizes| Dimension::length(sizes.max)),
    }
}

/// Picks the left-to-right answer or the right-to-left one.
fn flow<T>(rtl: bool, ltr: T, rtl_answer: T) -> T {
    if rtl { rtl_answer } else { ltr }
}

/// The plain lengths of a rect, per axis; percentages and `calc()` contribute nothing.
fn lengths_of(rect: &Rect<LengthPercentage>) -> Size<f32> {
    use taffy::ResolveOrZero;
    rect.resolve_or_zero(None::<f32>, |_, _| 0.0).sum_axes()
}

/// Which measurement a `width`-family value substitutes.
fn keyword_of_size(value: &SizeValue) -> Keyword {
    match value {
        SizeValue::MinContent => Keyword::Min,
        SizeValue::MaxContent | SizeValue::FitContent | SizeValue::FitContentFunction(_) => {
            Keyword::Max
        }
        _ => Keyword::None,
    }
}

/// Which measurement a `max-width`-family value substitutes.
fn keyword_of_max(value: &MaxSizeValue) -> Keyword {
    match value {
        MaxSizeValue::MinContent => Keyword::Min,
        MaxSizeValue::MaxContent
        | MaxSizeValue::FitContent
        | MaxSizeValue::FitContentFunction(_) => Keyword::Max,
        _ => Keyword::None,
    }
}

#[cfg(test)]
mod tests {
    use taffy::prelude::TaffyAuto;
    use taffy::{Dimension, Float, Rect};
    use zgui_css::StyleDraft;
    use zgui_css::values::length::{Length, LengthPercentage as CssLp, NonNegative};
    use zgui_css::values::size::{FloatValue, InsetValue, PositionValue, SizeValue};
    use zgui_css::values::text::Direction as CssDirection;

    use crate::style::calc::CalcTable;
    use crate::style::convert::length::IntrinsicSizes;
    use crate::style::{DeviceStyle, MeasuredSizes};

    use super::LayoutStyle;

    fn lower_with(mutate: impl FnOnce(&mut StyleDraft)) -> LayoutStyle {
        let mut draft = StyleDraft::initial();
        mutate(&mut draft);
        let style = draft.build();
        let mut calc = CalcTable::default();
        calc.set_scale(1.0);
        LayoutStyle::lower(&style, DeviceStyle::default(), &mut calc)
    }

    fn length(px: f32) -> CssLp {
        CssLp::new_length(Length::new(px))
    }

    #[test]
    fn a_content_keyword_substitutes_the_measurement_at_read_time() {
        let lowered = lower_with(|draft| {
            draft.position_group().width = SizeValue::MinContent;
            draft.position_group().min_height = SizeValue::MaxContent;
        });
        assert_eq!(
            lowered.size.width,
            Dimension::AUTO,
            "the packed value defers"
        );

        let unmeasured = lowered.size_with(MeasuredSizes::default());
        assert_eq!(unmeasured.width, Dimension::AUTO);

        let sizes = IntrinsicSizes {
            min: 30.0,
            max: 90.0,
        };
        let measured = MeasuredSizes {
            horizontal: Some(sizes),
            vertical: Some(sizes),
        };
        assert_eq!(lowered.size_with(measured).width, Dimension::length(30.0));
        assert_eq!(
            lowered.min_size_with(measured).height,
            Dimension::length(90.0)
        );
        assert_eq!(
            lowered.size_with(measured).height,
            Dimension::AUTO,
            "a keyword-free slot keeps its packed value"
        );
    }

    #[test]
    fn a_style_with_no_keywords_reads_the_packed_sizes_straight_through() {
        let lowered = lower_with(|draft| {
            draft.position_group().width = SizeValue::LengthPercentage(NonNegative(length(24.0)));
        });
        assert!(lowered.keywords.is_empty());
        let measured = MeasuredSizes {
            horizontal: Some(IntrinsicSizes { min: 1.0, max: 2.0 }),
            vertical: None,
        };
        assert_eq!(lowered.size_with(measured).width, Dimension::length(24.0));
    }

    #[test]
    fn a_static_box_has_no_inset_whatever_was_written() {
        let lowered = lower_with(|draft| {
            draft.position_group().left = InsetValue::LengthPercentage(length(10.0));
        });
        assert_eq!(lowered.inset, Rect::auto());

        let positioned = lower_with(|draft| {
            draft.box_group().position = PositionValue::Relative;
            draft.position_group().left = InsetValue::LengthPercentage(length(10.0));
        });
        assert_eq!(
            positioned.inset.left,
            taffy::LengthPercentageAuto::length(10.0)
        );
    }

    #[test]
    fn flow_relative_float_bakes_the_writing_direction() {
        let ltr = lower_with(|draft| {
            draft.box_group().float = FloatValue::InlineStart;
        });
        assert_eq!(ltr.float, Float::Left);

        let rtl = lower_with(|draft| {
            draft.box_group().float = FloatValue::InlineStart;
            draft.inherited_box().direction = CssDirection::Rtl;
        });
        assert_eq!(rtl.float, Float::Right);
    }

    #[test]
    fn content_box_padding_joins_the_intrinsic_inset_and_border_box_does_not() {
        let content_box = lower_with(|draft| {
            draft.padding().padding_left = NonNegative(length(10.0));
            draft.padding().padding_right = NonNegative(length(5.0));
        });
        assert_eq!(content_box.intrinsic_inset.width, 15.0);
        assert_eq!(content_box.intrinsic_inset.height, 0.0);

        let border_box = lower_with(|draft| {
            draft.padding().padding_left = NonNegative(length(10.0));
            draft.position_group().box_sizing = zgui_css::values::size::BoxSizingValue::BorderBox;
        });
        assert_eq!(border_box.intrinsic_inset.width, 0.0);
    }
}
