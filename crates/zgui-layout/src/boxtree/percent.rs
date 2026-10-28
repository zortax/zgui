//! Which inline formatting contexts hold an atomic inline with a percentage block size.
//!
//! A percentage `height`, `min-height` or `max-height` on an atomic inline resolves against the
//! block size of the containing block, which is the block container around the context root. The
//! root's own style has no percentage, so the layout engine drops that block size from the root's
//! layout question unless [`BoxNode::atomic_percent_block`] says otherwise.
//!
//! The bit is set when the tree is built, and kept by the patches that change a box's style or its
//! children inside an inline formatting context.

use zgui_css::ComputedStyle;
use zgui_css::values::size::{MaxSizeValue, SizeValue};
use zgui_dom::side::BoxKey;

use crate::node::box_node::BoxNode;
use crate::node::kind::FormattingContext;
use crate::tree::dirty::mark_dirty;
use crate::tree::store::LayoutStore;

/// Whether `style` sizes the block axis with a percentage of the containing block.
pub(crate) fn uses_percent_block_size(style: &ComputedStyle) -> bool {
    let position = style.get_position();
    size(&position.height) || size(&position.min_height) || max_size(&position.max_height)
}

/// Whether one `height` or `min-height` value reads a percentage.
fn size(value: &SizeValue) -> bool {
    match value {
        SizeValue::LengthPercentage(inner) | SizeValue::AnchorContainingCalcFunction(inner) => {
            inner.0.has_percentage()
        }
        SizeValue::FitContentFunction(limit) => limit.0.has_percentage(),
        SizeValue::WebkitFillAvailable | SizeValue::Stretch => true,
        SizeValue::Auto
        | SizeValue::AnchorSizeFunction(_)
        | SizeValue::MinContent
        | SizeValue::MaxContent
        | SizeValue::FitContent => false,
    }
}

/// Whether one `max-height` value reads a percentage.
fn max_size(value: &MaxSizeValue) -> bool {
    match value {
        MaxSizeValue::LengthPercentage(inner)
        | MaxSizeValue::AnchorContainingCalcFunction(inner) => inner.0.has_percentage(),
        MaxSizeValue::FitContentFunction(limit) => limit.0.has_percentage(),
        MaxSizeValue::WebkitFillAvailable | MaxSizeValue::Stretch => true,
        MaxSizeValue::None
        | MaxSizeValue::AnchorSizeFunction(_)
        | MaxSizeValue::MinContent
        | MaxSizeValue::MaxContent
        | MaxSizeValue::FitContent => false,
    }
}

/// What one box in an inline formatting context adds to its parent's bit.
///
/// An inline box or a text run passes on its own bit. Any other box is atomic, and its own style
/// decides.
fn contributes(node: &BoxNode) -> bool {
    if node.fc == FormattingContext::Inline {
        node.atomic_percent_block
    } else {
        uses_percent_block_size(&node.style)
    }
}

/// The bit of an inline box from its layout children.
pub(crate) fn of_children(store: &LayoutStore, children: &[BoxKey]) -> bool {
    children
        .iter()
        .any(|&child| store.get(child).is_some_and(contributes))
}

/// Computes the bit of `key` again, and of each inline ancestor whose bit follows from it.
///
/// Call it on a box whose children, or a child's style, changed. A box outside an inline
/// formatting context is left alone. When the bit of the context root changes, the root's layout
/// is thrown away, because its cached answers were keyed without the block size.
pub(crate) fn refresh(store: &mut LayoutStore, mut key: BoxKey) {
    loop {
        let Some(node) = store.get(key) else {
            return;
        };
        if node.fc != FormattingContext::Inline {
            return;
        }
        let bit = of_children(store, &node.children);
        if node.atomic_percent_block == bit {
            return;
        }
        // Written only when the bit moves: a write clears the box's paint-order memo.
        let node = store.get_mut(key).expect("a live box");
        node.atomic_percent_block = bit;
        match node.parent {
            Some(parent) if node.parent_fc == FormattingContext::Inline => key = parent,
            _ => {
                mark_dirty(store, key);
                return;
            }
        }
    }
}

/// Refreshes the inline context that holds `key`, when `key` is an atomic inline in one.
pub(crate) fn refresh_parent_of(store: &mut LayoutStore, key: BoxKey) {
    let Some(node) = store.get(key) else {
        return;
    };
    if node.parent_fc != FormattingContext::Inline || node.fc == FormattingContext::Inline {
        return;
    }
    let Some(parent) = node.parent else {
        return;
    };
    // A parent bit that agrees with this box's own contribution stays as it is, whatever the
    // siblings hold. This keeps a restyle of many atomics in one context linear.
    let own = uses_percent_block_size(&node.style);
    if store
        .get(parent)
        .is_some_and(|parent| parent.atomic_percent_block == own)
    {
        return;
    }
    refresh(store, parent);
}
