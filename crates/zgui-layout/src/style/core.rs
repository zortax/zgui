//! The properties every layout mode reads.

use taffy::prelude::TaffyZero;
use taffy::{
    BoxGenerationMode, BoxSizing, CoreStyle, Dimension, Direction, LengthPercentage,
    LengthPercentageAuto, Overflow, Point, Position, Rect, Size,
};
use zgui_css::values::size::{BoxSizingValue, PositionValue};
use zgui_interned::Ident;

use crate::node::kind::FormattingContext;
use crate::style::StyleRef;
use crate::style::convert::{aspect, length, overflow};

impl CoreStyle for StyleRef<'_> {
    type CustomIdent = Ident;

    fn box_generation_mode(&self) -> BoxGenerationMode {
        if self.generates_no_box() {
            BoxGenerationMode::None
        } else {
            BoxGenerationMode::Normal
        }
    }

    fn is_block(&self) -> bool {
        // A box that holds lines is a block-level box that happens to lay its children out on
        // lines, and it takes part in the block formatting context around it exactly as a block
        // container does: its margins collapse with its siblings', and a float beside it shortens
        // its *lines* rather than moving the whole of it out of the way.
        match self.node().fc {
            FormattingContext::Block => true,
            FormattingContext::Inline => self.node().block_level,
            _ => false,
        }
    }

    fn is_compressible_replaced(&self) -> bool {
        self.node().fc == FormattingContext::Replaced
    }

    fn box_sizing(&self) -> BoxSizing {
        match self.position_group().box_sizing {
            BoxSizingValue::ContentBox => BoxSizing::ContentBox,
            BoxSizingValue::BorderBox => BoxSizing::BorderBox,
        }
    }

    fn direction(&self) -> Direction {
        if self.is_rtl() {
            Direction::Rtl
        } else {
            Direction::Ltr
        }
    }

    fn overflow(&self) -> Point<Overflow> {
        let reserved = self.reserved_gutter();
        Point {
            x: overflow::decided(self.box_().overflow_x, reserved.0),
            y: overflow::decided(self.box_().overflow_y, reserved.1),
        }
    }

    fn scrollbar_width(&self) -> f32 {
        self.device_scrollbar_width()
    }

    fn position(&self) -> Position {
        match self.box_().position {
            // A static box has no inset at all, which is why `inset` below reports none for it
            // rather than reporting what was written and letting it move the box.
            PositionValue::Static | PositionValue::Relative | PositionValue::Sticky => {
                Position::Relative
            }
            PositionValue::Absolute | PositionValue::Fixed => Position::Absolute,
        }
    }

    fn is_fixed_position(&self) -> bool {
        // Laid out as absolute above, told apart here for one thing only: a fixed box does not
        // scroll with anything, so it contributes no scrollable overflow to its containing block.
        // Without this, the viewport-sized overlay root would hand the page's scroller a sideways
        // scrollbar the moment the page reserved a gutter.
        self.box_().position == PositionValue::Fixed
    }

    fn inset(&self) -> Rect<LengthPercentageAuto> {
        if self.box_().position == PositionValue::Static {
            return Rect::auto();
        }
        let (scale, calc) = (self.scale(), self.calc());
        let position = self.position_group();
        Rect {
            left: length::inset(&position.left, scale, calc),
            right: length::inset(&position.right, scale, calc),
            top: length::inset(&position.top, scale, calc),
            bottom: length::inset(&position.bottom, scale, calc),
        }
    }

    fn size(&self) -> Size<Dimension> {
        let (scale, calc, measured) = (self.scale(), self.calc(), self.measured());
        let position = self.position_group();
        Size {
            width: length::size(&position.width, scale, calc, measured.horizontal),
            height: length::size(&position.height, scale, calc, measured.vertical),
        }
    }

    fn min_size(&self) -> Size<Dimension> {
        let (scale, calc, measured) = (self.scale(), self.calc(), self.measured());
        let position = self.position_group();
        Size {
            width: length::size(&position.min_width, scale, calc, measured.horizontal),
            height: length::size(&position.min_height, scale, calc, measured.vertical),
        }
    }

    fn max_size(&self) -> Size<Dimension> {
        let (scale, calc, measured) = (self.scale(), self.calc(), self.measured());
        let position = self.position_group();
        Size {
            width: length::max_size(&position.max_width, scale, calc, measured.horizontal),
            height: length::max_size(&position.max_height, scale, calc, measured.vertical),
        }
    }

    fn aspect_ratio(&self) -> Option<f32> {
        aspect::aspect_ratio(&self.position_group().aspect_ratio, self.natural_ratio())
    }

    fn margin(&self) -> Rect<LengthPercentageAuto> {
        let (scale, calc) = (self.scale(), self.calc());
        let margin = self.style().get_margin();
        Rect {
            left: length::margin(&margin.margin_left, scale, calc),
            right: length::margin(&margin.margin_right, scale, calc),
            top: length::margin(&margin.margin_top, scale, calc),
            bottom: length::margin(&margin.margin_bottom, scale, calc),
        }
    }

    fn padding(&self) -> Rect<LengthPercentage> {
        let (scale, calc) = (self.scale(), self.calc());
        let padding = self.style().get_padding();
        Rect {
            left: length::padding(&padding.padding_left, scale, calc),
            right: length::padding(&padding.padding_right, scale, calc),
            top: length::padding(&padding.padding_top, scale, calc),
            bottom: length::padding(&padding.padding_bottom, scale, calc),
        }
    }

    fn border(&self) -> Rect<LengthPercentage> {
        // A side whose style is `none` or `hidden` has no border at all, whatever width was
        // written. The computed width is the written one, so the suppression happens here — and it
        // is not cosmetic: the initial width is three pixels, so a box with no border at all would
        // otherwise lose six pixels of content box on each axis.
        let scale = self.scale();
        let border = self.style().get_border();
        Rect {
            left: side(&border.border_left_width, border.border_left_style, scale),
            right: side(&border.border_right_width, border.border_right_style, scale),
            top: side(&border.border_top_width, border.border_top_style, scale),
            bottom: side(
                &border.border_bottom_width,
                border.border_bottom_style,
                scale,
            ),
        }
    }
}

/// One border side's width, in device pixels, or zero if that side draws nothing.
fn side(
    width: &zgui_css::values::border::BorderSideWidthValue,
    style: zgui_css::values::border::BorderStyleValue,
    scale: f32,
) -> LengthPercentage {
    match style {
        zgui_css::values::border::BorderStyleValue::None
        | zgui_css::values::border::BorderStyleValue::Hidden => LengthPercentage::ZERO,
        _ => LengthPercentage::length(width.0.to_f32_px() * scale),
    }
}
