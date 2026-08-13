//! The computed style, as the layout algorithms see it.
//!
//! No layout-engine style struct is built per box. The algorithms read styles through traits, and
//! what implements those traits is [`StyleRef`] — a borrow of one box and its interned
//! [`lowered::LayoutStyle`], plus whatever varies per box: the intrinsic pre-pass measurements,
//! a replaced box's natural ratio, and the gutters layout has decided. It is [`Copy`], it holds
//! no lock and no guard, and it is dropped before any recursion, which is what the algorithms
//! require of it: they interleave shared style reads with exclusive recursion into the tree, and
//! anything holding a borrow across that boundary deadlocks or fails to compile.

pub mod block;
pub mod calc;
pub mod convert;
pub mod core;
pub mod flex;
pub mod gap;
pub mod grid;
pub(crate) mod lowered;

use zgui_css::{ComputedStyle, computed::style::style_structs};

use crate::node::box_node::BoxNode;
use crate::node::kind::FormattingContext;
use crate::style::convert::length::IntrinsicSizes;
use crate::style::lowered::LayoutStyle;

/// Whether two computed styles are the same cascade result, as opposed to two that merely agree.
///
/// Allocation identity rather than value equality. A cascade result is a fresh allocation each time
/// it is computed and shared by every element it was computed for, so this is a pointer comparison
/// on the path a document full of similar elements takes. Two styles that do not share an
/// allocation may still agree on every property, and treating those as different costs one refcount
/// and no downstream work — every consumer keys on the property groups rather than on the style as
/// a whole.
pub(crate) fn same_cascade(held: &ComputedStyle, style: &ComputedStyle) -> bool {
    ::core::ptr::eq(
        ::core::ptr::from_ref(&**held),
        ::core::ptr::from_ref(&**style),
    )
}

/// The numbers a layout pass supplies that no style carries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeviceStyle {
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// How wide a scrollbar is, in device pixels.
    ///
    /// This is not a CSS property in this build — the longhand that would carry it is generated
    /// only for another engine — so it comes from the theme.
    pub scrollbar_width: f32,
}

impl Default for DeviceStyle {
    fn default() -> Self {
        Self {
            scale: 1.0,
            scrollbar_width: 15.0,
        }
    }
}

/// What the intrinsic pre-pass measured for one box, if it measured anything.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeasuredSizes {
    /// The horizontal minimum and maximum.
    pub horizontal: Option<IntrinsicSizes>,
    /// The vertical minimum and maximum.
    pub vertical: Option<IntrinsicSizes>,
}

/// A borrowed view over one box's computed style.
///
/// Copying one costs three words and no allocation, and dropping one releases nothing, which is
/// what makes it safe to hand to an algorithm that will immediately recurse.
#[derive(Clone, Copy)]
pub struct StyleRef<'a> {
    /// The box whose style this is.
    node: &'a BoxNode,
    /// The box's style, lowered once into the layout algorithms' vocabulary.
    lowered: &'a LayoutStyle,
    /// The numbers no style carries.
    device: DeviceStyle,
    /// What the intrinsic pre-pass measured for this box.
    measured: MeasuredSizes,
    /// The natural proportions of this box's content, if it has any.
    natural_ratio: Option<f32>,
    /// Which axes layout itself has decided reserve a scrollbar gutter.
    ///
    /// `overflow: auto` reserves a gutter exactly when its content overflows, which is not known
    /// until the box has been laid out, and a locked scroll container keeps the gutter it had
    /// whatever its style now says. Neither is in the computed style, so both arrive here.
    reserved_gutter: (bool, bool),
}

impl<'a> StyleRef<'a> {
    /// A view over `node`'s style.
    pub(crate) fn new(
        node: &'a BoxNode,
        lowered: &'a LayoutStyle,
        device: DeviceStyle,
        measured: MeasuredSizes,
        natural_ratio: Option<f32>,
    ) -> Self {
        Self {
            node,
            lowered,
            device,
            measured,
            natural_ratio,
            reserved_gutter: (false, false),
        }
    }

    /// The same view, told which axes layout has decided reserve a gutter.
    #[must_use]
    pub fn with_reserved_gutter(mut self, axes: (bool, bool)) -> Self {
        self.reserved_gutter = axes;
        self
    }

    /// Which axes reserve a gutter by layout's own decision rather than by what was written.
    pub(crate) fn reserved_gutter(self) -> (bool, bool) {
        self.reserved_gutter
    }

    /// The box this is a view over.
    pub fn node(self) -> &'a BoxNode {
        self.node
    }

    /// The computed style itself.
    pub fn style(self) -> &'a ComputedStyle {
        &self.node.style
    }

    /// The `position` property group, which carries every sizing and alignment property.
    ///
    /// Named for the group rather than for the property, because one of the layout traits also has
    /// a `position` method and it answers a different question.
    pub(crate) fn position_group(self) -> &'a style_structs::Position {
        self.node.style.get_position()
    }

    /// Device pixels per CSS pixel.
    pub(crate) fn scale(self) -> f32 {
        self.device.scale
    }

    /// How wide a scrollbar is, in device pixels.
    pub(crate) fn device_scrollbar_width(self) -> f32 {
        self.device.scrollbar_width
    }

    /// The style in the layout algorithms' vocabulary.
    pub(crate) fn lowered(self) -> &'a LayoutStyle {
        self.lowered
    }

    /// What the intrinsic pre-pass measured, stated the way a size in the style is stated.
    ///
    /// The measurement is of the whole box — the padding and the border are inside the number that
    /// came back. A size written in the style is the *content* box unless `box-sizing` says
    /// otherwise, and the layout algorithms add the padding and border back on to whatever the
    /// style asked for. So under `box-sizing: content-box` they come off here — the lowering
    /// carries the amount — or every box sized by a content keyword ends up one padding and border
    /// wider and taller than its own content.
    pub(crate) fn measured(self) -> MeasuredSizes {
        let inset = self.lowered.intrinsic_inset;
        MeasuredSizes {
            horizontal: self
                .measured
                .horizontal
                .map(|sizes| sizes.less(inset.width)),
            vertical: self.measured.vertical.map(|sizes| sizes.less(inset.height)),
        }
    }

    /// The natural proportions of this box's content.
    pub(crate) fn natural_ratio(self) -> Option<f32> {
        self.natural_ratio
    }

    /// Whether this box generates no geometry at all.
    ///
    /// `display: none` is one way; `visibility: collapse` on a flex item is the other, and it is
    /// not a paint-time property — a collapsed item is removed from its container's layout the way
    /// a removed row is removed from a table.
    pub(crate) fn generates_no_box(self) -> bool {
        self.node.fc == FormattingContext::None
            || (self.lowered.collapses_as_flex_item && self.is_flex_item())
    }

    /// Whether this box's parent lays it out by flex rules.
    fn is_flex_item(self) -> bool {
        self.node.parent_fc == FormattingContext::Flex
    }
}
