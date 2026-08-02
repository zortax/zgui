//! Stacking contexts, and the order the document is painted in.
//!
//! Painting order is not document order and it is not the box tree's order either. A document is
//! painted as a forest of *stacking contexts*, and inside each one the contents are painted in
//! seven passes: negative stacking children, then block-level backgrounds, then floats, then inline
//! content, then positioned and zero-index children, then positive stacking children. A box that
//! establishes a context of its own is painted atomically, wherever the context sits in its
//! parent's sequence, which is why an `opacity: 0.99` on an ancestor can move a whole subtree in
//! front of content it used to sit behind.
//!
//! Two things follow, and both are load-bearing elsewhere. The order computed here is the order the
//! display list is emitted in, so it is also the order the hit-test index carries — a hit is the
//! last thing painted under the point, and hit order and paint order cannot be allowed to diverge.
//! And a stacking context is named by the box that establishes it, so its identifier is stable from
//! frame to frame without anything having to allocate one.

use zgui_css::values::effect::{IsolationValue, MixBlendModeValue};
use zgui_css::values::size::{PositionValue, ZIndexValue};
use zgui_dom::side::BoxKey;
use zgui_scene::StackingContextId;

use crate::node::kind::FormattingContext;
use crate::tree::store::LayoutStore;

/// Which of Appendix E's passes a box is painted in, inside its stacking context.
///
/// The order of the variants *is* the painting order, so a sort by this value is a sort into CSS
/// painting order and nothing else has to know the sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PaintLevel {
    /// A stacking context with a negative `z-index`.
    NegativeStacking,
    /// A block-level box in the normal flow.
    Block,
    /// A floated box.
    Float,
    /// An inline-level box in the normal flow.
    Inline,
    /// A positioned box, or a stacking context, whose `z-index` is `auto` or zero.
    Positioned,
    /// A stacking context with a positive `z-index`.
    PositiveStacking,
}

/// Whether a box establishes a stacking context of its own.
///
/// The list is CSS's, and every entry on it is a property that makes the subtree composite as a
/// unit: a positioned box with a `z-index`, anything transparent, transformed, filtered, blended or
/// isolated, and a flex or grid item that gave itself a `z-index`.
pub fn establishes(store: &LayoutStore, key: BoxKey) -> bool {
    let Some(node) = store.get(key) else {
        return false;
    };
    if node.parent.is_none() {
        // The root box's context is the one everything else is painted into.
        return true;
    }
    let style = &node.style;
    let box_ = style.get_box();
    let effects = style.get_effects();
    let positioned = box_.position != PositionValue::Static;
    let indexed = !matches!(style.get_position().z_index, ZIndexValue::Auto);

    if positioned && indexed {
        return true;
    }
    if box_.position == PositionValue::Fixed || box_.position == PositionValue::Sticky {
        return true;
    }
    if indexed
        && matches!(
            node.parent_fc,
            FormattingContext::Flex | FormattingContext::Grid
        )
    {
        return true;
    }
    effects.opacity < 1.0
        || effects.mix_blend_mode != MixBlendModeValue::Normal
        || box_.isolation == IsolationValue::Isolate
        || !effects.filter.0.is_empty()
        || !effects.backdrop_filter.0.is_empty()
        || !matches!(
            style.get_svg().clip_path,
            zgui_css::values::effect::ClipPathValue::None
        )
        || crate::fragment::transform::is_transformed(box_)
}

/// The identifier of the stacking context a box establishes.
///
/// Derived from the box's own name rather than issued by a counter, so it is the same identifier
/// every frame for as long as the box lives — which is what lets a walk that visits only part of
/// the document leave the rest of the fragment tree's context identifiers alone.
pub fn id_of(key: BoxKey) -> StackingContextId {
    StackingContextId(key.index())
}

/// Which pass a box is painted in, inside the context that contains it.
pub fn level(store: &LayoutStore, key: BoxKey) -> PaintLevel {
    let Some(node) = store.get(key) else {
        return PaintLevel::Block;
    };
    let style = &node.style;
    if establishes(store, key) || style.get_box().position != PositionValue::Static {
        return match style.get_position().z_index {
            ZIndexValue::Integer(index) if index < 0 => PaintLevel::NegativeStacking,
            ZIndexValue::Integer(index) if index > 0 => PaintLevel::PositiveStacking,
            _ => PaintLevel::Positioned,
        };
    }
    if style.get_box().float != zgui_css::values::size::FloatValue::None {
        return PaintLevel::Float;
    }
    if node.block_level {
        PaintLevel::Block
    } else {
        PaintLevel::Inline
    }
}

/// The `z-index` a box sorts by within its pass, which is zero for everything unindexed.
pub fn z_index(store: &LayoutStore, key: BoxKey) -> i32 {
    match store.get(key).map(|node| node.style.get_position().z_index) {
        Some(ZIndexValue::Integer(index)) => index,
        _ => 0,
    }
}

/// Every box below `root`, in the order the document paints them, `root` first.
///
/// The walk descends the layout child list, which is the list that reaches every box exactly once:
/// an anonymous wrapper is in it and in no document order, and an out-of-flow box is in the list of
/// the box that positions it rather than of the one that declared it, which is where it is painted.
/// At each box the children are sorted into Appendix E's passes; the sort is stable, so two children
/// in the same pass with the same `z-index` keep the order they are laid out in, which is the
/// tie-break the specification gives — and which `order` on a flex item has already moved, exactly
/// as it moves painting.
pub fn paint_order(store: &LayoutStore, root: BoxKey) -> Vec<BoxKey> {
    let mut order = Vec::new();
    push_subtree(store, root, &mut order);
    order
}

/// Appends one box and everything below it, in painting order.
fn push_subtree(store: &LayoutStore, key: BoxKey, order: &mut Vec<BoxKey>) {
    order.push(key);
    let Some(node) = store.get(key) else {
        return;
    };
    let mut children: Vec<(PaintLevel, i32, usize, BoxKey)> = node
        .children
        .iter()
        .enumerate()
        .map(|(position, &child)| (level(store, child), z_index(store, child), position, child))
        .collect();
    children.sort_by_key(|(level, index, position, _)| (*level, *index, *position));
    for (_, _, _, child) in children {
        push_subtree(store, child, order);
    }
}

#[cfg(test)]
mod tests {
    use super::PaintLevel;

    #[test]
    fn the_passes_are_ordered_the_way_the_document_is_painted() {
        let mut passes = [
            PaintLevel::Positioned,
            PaintLevel::Float,
            PaintLevel::PositiveStacking,
            PaintLevel::Block,
            PaintLevel::NegativeStacking,
            PaintLevel::Inline,
        ];
        passes.sort();
        assert_eq!(
            passes,
            [
                PaintLevel::NegativeStacking,
                PaintLevel::Block,
                PaintLevel::Float,
                PaintLevel::Inline,
                PaintLevel::Positioned,
                PaintLevel::PositiveStacking,
            ]
        );
    }
}
