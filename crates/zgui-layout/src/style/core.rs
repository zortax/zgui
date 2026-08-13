//! The properties every layout mode reads.

use taffy::{
    BoxGenerationMode, BoxSizing, CoreStyle, Dimension, Direction, LengthPercentage,
    LengthPercentageAuto, Overflow, Point, Position, Rect, Size,
};
use zgui_interned::Ident;

use crate::node::kind::FormattingContext;
use crate::style::StyleRef;

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
        self.lowered().box_sizing
    }

    fn direction(&self) -> Direction {
        self.lowered().direction
    }

    fn overflow(&self) -> Point<Overflow> {
        // An axis layout has decided reserves a gutter is `scroll` to the algorithms whatever the
        // style says, because reserving the space is what that value means to them.
        let reserved = self.reserved_gutter();
        let base = self.lowered().overflow;
        Point {
            x: if reserved.0 { Overflow::Scroll } else { base.x },
            y: if reserved.1 { Overflow::Scroll } else { base.y },
        }
    }

    fn scrollbar_width(&self) -> f32 {
        self.device_scrollbar_width()
    }

    fn position(&self) -> Position {
        self.lowered().position
    }

    fn is_fixed_position(&self) -> bool {
        // Laid out as absolute, told apart here for one thing only: a fixed box does not scroll
        // with anything, so it contributes no scrollable overflow to its containing block.
        self.lowered().fixed
    }

    fn inset(&self) -> Rect<LengthPercentageAuto> {
        self.lowered().inset
    }

    fn size(&self) -> Size<Dimension> {
        self.lowered().size_with(self.measured())
    }

    fn min_size(&self) -> Size<Dimension> {
        self.lowered().min_size_with(self.measured())
    }

    fn max_size(&self) -> Size<Dimension> {
        self.lowered().max_size_with(self.measured())
    }

    fn aspect_ratio(&self) -> Option<f32> {
        self.lowered().aspect_ratio(self.natural_ratio())
    }

    fn margin(&self) -> Rect<LengthPercentageAuto> {
        self.lowered().margin
    }

    fn padding(&self) -> Rect<LengthPercentage> {
        self.lowered().padding
    }

    fn border(&self) -> Rect<LengthPercentage> {
        self.lowered().border
    }
}
