//! The computed style, as the layout algorithms see it.
//!
//! No layout-engine style struct is ever built. The algorithms read styles through traits, and what
//! implements those traits is [`StyleRef`] — a borrow of one box's computed style, two numbers the
//! document supplies, and whatever the intrinsic pre-pass measured. It is [`Copy`], it holds no
//! lock and no guard, and it is dropped before any recursion, which is what the algorithms require
//! of it: they interleave shared style reads with exclusive recursion into the tree, and anything
//! holding a borrow across that boundary deadlocks or fails to compile.

pub mod block;
pub mod calc;
pub mod convert;
pub mod core;
pub mod flex;
pub mod gap;
pub mod grid;

use ::core::cell::RefCell;

use zgui_css::values::size::{BoxSizingValue, DisplayOutside, VisibilityValue};
use zgui_css::{ComputedStyle, computed::style::style_structs};

use crate::node::box_node::BoxNode;
use crate::node::kind::FormattingContext;
use crate::style::calc::CalcArena;
use crate::style::convert::length::IntrinsicSizes;

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
    /// Where `calc()` expressions are interned for this pass.
    calc: &'a RefCell<CalcArena>,
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
    pub fn new(
        node: &'a BoxNode,
        calc: &'a RefCell<CalcArena>,
        device: DeviceStyle,
        measured: MeasuredSizes,
        natural_ratio: Option<f32>,
    ) -> Self {
        Self {
            node,
            calc,
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

    /// The `box` property group.
    pub(crate) fn box_(self) -> &'a style_structs::Box {
        self.node.style.get_box()
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

    /// Where this pass interns `calc()`.
    pub(crate) fn calc(self) -> &'a RefCell<CalcArena> {
        self.calc
    }

    /// What the intrinsic pre-pass measured, stated the way a size in the style is stated.
    ///
    /// The measurement is of the whole box — the padding and the border are inside the number that
    /// came back. A size written in the style is the *content* box unless `box-sizing` says
    /// otherwise, and the layout algorithms add the padding and border back on to whatever the
    /// style asked for. So under `box-sizing: content-box` they come off here, or every box sized
    /// by a content keyword ends up one padding and border wider and taller than its own content.
    ///
    /// Percentage padding resolves against no basis, which is nothing: a percentage inset
    /// contributes nothing to an intrinsic size, because the size it would resolve against is the
    /// one being computed.
    pub(crate) fn measured(self) -> MeasuredSizes {
        let inset = self.intrinsic_inset();
        MeasuredSizes {
            horizontal: self
                .measured
                .horizontal
                .map(|sizes| sizes.less(inset.width)),
            vertical: self.measured.vertical.map(|sizes| sizes.less(inset.height)),
        }
    }

    /// How much of a measurement is this box's own padding and border, as far as a size written in
    /// the style is concerned.
    fn intrinsic_inset(self) -> taffy::Size<f32> {
        use taffy::{CoreStyle, ResolveOrZero};

        if self.node.style.get_position().box_sizing != BoxSizingValue::ContentBox {
            return taffy::Size {
                width: 0.0,
                height: 0.0,
            };
        }
        let resolve =
            |value: *const (), basis: f32| crate::style::calc::resolve_in(self.calc, value, basis);
        let padding = self.padding().resolve_or_zero(None::<f32>, resolve);
        let border = self.border().resolve_or_zero(None::<f32>, resolve);
        (padding + border).sum_axes()
    }

    /// The natural proportions of this box's content.
    pub(crate) fn natural_ratio(self) -> Option<f32> {
        self.natural_ratio
    }

    /// Whether text runs right to left in this box.
    pub(crate) fn is_rtl(self) -> bool {
        self.node.style.get_inherited_box().direction == zgui_css::values::text::Direction::Rtl
    }

    /// Whether this box generates no geometry at all.
    ///
    /// `display: none` is one way; `visibility: collapse` on a flex item is the other, and it is
    /// not a paint-time property — a collapsed item is removed from its container's layout the way
    /// a removed row is removed from a table.
    pub(crate) fn generates_no_box(self) -> bool {
        self.node.fc == FormattingContext::None
            || (self.node.style.get_inherited_box().visibility == VisibilityValue::Collapse
                && self.node.style.get_box().display.outside() != DisplayOutside::None
                && self.is_flex_item())
    }

    /// Whether this box's parent lays it out by flex rules.
    fn is_flex_item(self) -> bool {
        self.node.parent_fc == FormattingContext::Flex
    }
}
