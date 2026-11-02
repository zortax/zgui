//! One box of the box tree.

use core::sync::atomic::{AtomicBool, Ordering};

use zgui_css::ComputedStyle;
use zgui_dom::NodeKey;
use zgui_dom::side::BoxKey;

use crate::node::kind::{BoxKind, FormattingContext, PaintedContent, PseudoKind};

/// One box in the box tree.
///
/// Boxes are generated from nodes but are not one-to-one with them: a hidden element generates
/// none, an element whose children reparent onto its own parent generates none, a run of inline
/// siblings shares one anonymous box, and generated content produces boxes with no element behind
/// them at all.
///
/// The two child lists are different lists with different orders and they are both needed. The
/// layout order is what the layout algorithms walk, with `order` already applied, flattening
/// already done and out-of-flow boxes already moved onto the containing block that positions them.
/// The paint order is document order, which is what painting, hit testing and accessible geometry
/// need. Conflating them corrupts one of the two.
#[derive(Clone, Debug)]
pub struct BoxNode {
    /// The element this box was generated from, or nothing for an anonymous box.
    ///
    /// A generated-content box names the element it was generated *from*, not a element of its
    /// own, because it has none.
    pub source: Option<NodeKey>,
    /// Which generated-content pseudo-element this box realises, if any.
    pub pseudo: Option<PseudoKind>,
    /// The box this one is a child of, or nothing for the root.
    pub parent: Option<BoxKey>,
    /// Children in layout order.
    pub children: Vec<BoxKey>,
    /// Children in document order.
    pub paint_children: Vec<BoxKey>,
    /// The rules this box lays its children out by.
    pub fc: FormattingContext,
    /// The rules the box *containing* this one lays it out by.
    ///
    /// Recorded rather than looked up, because several decisions depend on it — `visibility:
    /// collapse` removes a flex item's box but only hides anything else, and an item's automatic
    /// minimum size exists in flex and grid and not in block flow — and none of them can reach the
    /// parent from a style.
    pub parent_fc: FormattingContext,
    /// The style this box is laid out with.
    ///
    /// Shared with the source element. An anonymous box gets a synthesised style whose inherited
    /// properties come from its parent and whose other properties are at their initial values.
    pub style: ComputedStyle,
    /// What this box is.
    pub kind: BoxKind,
    /// Whether this box is block-level in the context around it.
    ///
    /// Separate from [`BoxNode::fc`], which says what happens *inside* the box: an `inline-block`
    /// is not block-level and lays its children out by block rules, and an anonymous wrapper is
    /// block-level and establishes an inline formatting context.
    pub block_level: bool,
    /// The text this box lays out, for a text run or a list item's mark.
    pub text: Option<Box<str>>,
    /// What this box's own fragment paints. Rare payloads live in sparse store columns.
    pub painted: PaintedContent,
    /// On a box of an inline formatting context: whether an atomic inline in its content sizes
    /// its block axis with a percentage.
    ///
    /// Such a percentage resolves against the block size of the context root's containing block,
    /// so the root keeps that size in its layout question. The box tree keeps it current when it
    /// builds and patches boxes.
    pub atomic_percent_block: bool,
    /// Whether the children are known to paint in layout order.
    ///
    /// See [`LayoutOrderMemo`].
    pub(crate) layout_order: LayoutOrderMemo,
}

/// A remembered "yes" from
/// [`children_in_layout_order`](crate::fragment::stacking::children_in_layout_order).
///
/// The store clears it on each write that can change the answer: a write through
/// [`LayoutStore::get_mut`](crate::tree::store::LayoutStore::get_mut) to the box or to one of its
/// children, a removed child, a changed out-of-flow record, and a child style that changes
/// [`sort_inputs`](crate::fragment::stacking::sort_inputs). Only an answer that reads nothing but
/// the box and its children is kept. A child whose parent is another box, or an absolute or fixed
/// child with an out-of-flow record, keeps the memo clear.
///
/// A removed box must leave its parent's child list in the same write phase: after
/// [`LayoutStore::recycle`](crate::tree::store::LayoutStore::recycle) its record is gone and its
/// key sorts differently.
///
/// Atomic, so a reader that holds the store shared can record the answer. Each write that clears
/// it holds the store exclusively. A copy of the box starts clear.
#[derive(Debug, Default)]
pub(crate) struct LayoutOrderMemo(AtomicBool);

impl LayoutOrderMemo {
    /// Whether a "yes" is held.
    pub(crate) fn known(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Holds a "yes".
    pub(crate) fn remember(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Drops the held answer.
    pub(crate) fn clear(&mut self) {
        *self.0.get_mut() = false;
    }
}

impl Clone for LayoutOrderMemo {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl BoxNode {
    /// A childless box with the given style, kind and formatting context.
    pub fn new(style: ComputedStyle, kind: BoxKind, fc: FormattingContext) -> Self {
        Self {
            source: None,
            pseudo: None,
            parent: None,
            children: Vec::new(),
            paint_children: Vec::new(),
            fc,
            parent_fc: FormattingContext::Block,
            style,
            kind,
            block_level: false,
            text: None,
            painted: PaintedContent::Box,
            atomic_percent_block: false,
            layout_order: LayoutOrderMemo::default(),
        }
    }

    /// The same box, attributed to the element it was generated from.
    #[must_use]
    pub fn from_element(mut self, source: NodeKey) -> Self {
        self.source = Some(source);
        self
    }

    /// The same box, marked as realising one of its element's generated-content pseudo-elements.
    #[must_use]
    pub fn as_pseudo(mut self, pseudo: PseudoKind) -> Self {
        self.pseudo = Some(pseudo);
        self
    }

    /// The same box, carrying text.
    #[must_use]
    pub fn with_text(mut self, text: impl Into<Box<str>>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Whether this box has no children in either order.
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty() && self.paint_children.is_empty()
    }

    /// Whether this is a nested inline box: a box inside an inline formatting context that is
    /// not an atomic inline.
    ///
    /// Such a box has no rectangle of its own size. The lines it is on paint its parts.
    pub fn is_nested_inline(&self) -> bool {
        self.parent_fc == FormattingContext::Inline
            && !matches!(
                self.fc,
                FormattingContext::Atomic | FormattingContext::Replaced | FormattingContext::Custom
            )
    }
}
