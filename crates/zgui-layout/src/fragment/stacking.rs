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

use std::borrow::Cow;
use zgui_css::values::effect::{IsolationValue, MixBlendModeValue};
use zgui_css::values::size::{PositionValue, ZIndexValue};
use zgui_dom::side::BoxKey;
use zgui_scene::StackingContextId;

use crate::node::box_node::BoxNode;
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
    store.get(key).is_some_and(establishes_node)
}

/// [`establishes`], for a box already in hand.
fn establishes_node(node: &BoxNode) -> bool {
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
    store.get(key).map_or(PaintLevel::Block, level_of)
}

/// [`level`], for a box already in hand.
fn level_of(node: &BoxNode) -> PaintLevel {
    let style = &node.style;
    if establishes_node(node) || style.get_box().position != PositionValue::Static {
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
    store.get(key).map_or(0, z_of)
}

/// [`z_index`], for a box already in hand.
fn z_of(node: &BoxNode) -> i32 {
    match node.style.get_position().z_index {
        ZIndexValue::Integer(index) => index,
        _ => 0,
    }
}

/// What one child sorts by among its siblings: pass, then `z-index`, then tree order.
///
/// The field order is the sort order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PaintKey {
    /// The [`PaintLevel`] the child paints in.
    pub pass: u8,
    /// Its `z-index`, zero when it has none.
    pub z: i32,
    /// Its place in tree order, see [`tree_rank`].
    pub rank: (u32, u8),
}

/// The key `child`, at `position` in `parent`'s layout child list, sorts by among its siblings.
pub fn paint_key(
    store: &LayoutStore,
    parent: BoxKey,
    parent_node: &BoxNode,
    position: usize,
    child: BoxKey,
) -> PaintKey {
    match store.get(child) {
        Some(node) => PaintKey {
            pass: level_of(node) as u8,
            z: z_of(node),
            rank: rank_of(store, parent, parent_node, position, child, node),
        },
        None => PaintKey {
            pass: PaintLevel::Block as u8,
            z: 0,
            rank: in_flow(position),
        },
    }
}

/// Where `child`, at `position` in `parent`'s layout child list, sits in tree order.
///
/// An in-flow child at index `i` ranks `(i + 1, 0)`. An out-of-flow box is appended to the list of
/// the box that positions it, wherever it was written, so it ranks after the layout child it was
/// written after: `(j + 1, 1)` for the child at index `j`, and `(0, 1)` when it was written before
/// every in-flow child. A box written inside a deeper descendant ranks after the layout child that
/// holds that descendant. Equal ranks keep the order of the list.
pub fn tree_rank(
    store: &LayoutStore,
    parent: BoxKey,
    parent_node: &BoxNode,
    position: usize,
    child: BoxKey,
) -> (u32, u8) {
    match store.get(child) {
        Some(node) => rank_of(store, parent, parent_node, position, child, node),
        None => in_flow(position),
    }
}

/// The rank of an in-flow child at `position`.
fn in_flow(position: usize) -> (u32, u8) {
    (u32::try_from(position).unwrap_or(u32::MAX - 1) + 1, 0)
}

/// [`tree_rank`], for a child already in hand.
fn rank_of(
    store: &LayoutStore,
    parent: BoxKey,
    parent_node: &BoxNode,
    position: usize,
    child: BoxKey,
    node: &BoxNode,
) -> (u32, u8) {
    // Only an absolutely positioned box is re-parented, so every other child is answered without a
    // lookup.
    if !matches!(
        node.style.get_box().position,
        PositionValue::Absolute | PositionValue::Fixed
    ) {
        return in_flow(position);
    }
    let Some(declared) = store.declared(child) else {
        return in_flow(position);
    };
    let after = |index: u32| (index + 1, 1);
    if declared.parent != parent {
        return layout_index(store, parent, parent_node, declared.parent)
            .map_or_else(|| in_flow(position), after);
    }
    if let Some(anchor) = declared.after
        && let Some(index) = layout_index(store, parent, parent_node, anchor)
    {
        return after(index);
    }
    if declared.after.is_none() {
        return (0, 1);
    }
    // The sibling it was written after is gone: search the paint list for the nearest in-flow one.
    let Some(at) = parent_node
        .paint_children
        .iter()
        .position(|&it| it == child)
    else {
        return in_flow(position);
    };
    parent_node.paint_children[..at]
        .iter()
        .rev()
        .filter(|&&sibling| store.declared(sibling).is_none())
        .find_map(|&sibling| layout_index(store, parent, parent_node, sibling))
        .map_or((0, 1), after)
}

/// The index in `parent`'s layout child list of the child that holds `key`.
///
/// `key` is that child, or a box below it: an anonymous wrapper or a static descendant.
fn layout_index(
    store: &LayoutStore,
    parent: BoxKey,
    parent_node: &BoxNode,
    key: BoxKey,
) -> Option<u32> {
    let mut current = key;
    loop {
        let up = store.get(current)?.parent?;
        if up == parent {
            break;
        }
        current = up;
    }
    let index = parent_node.children.iter().position(|&it| it == current)?;
    u32::try_from(index).ok()
}

/// Every box below `root`, in the order the document paints them, `root` first.
///
/// The walk descends the layout child list, which is the list that reaches every box exactly once:
/// an anonymous wrapper is in it and in no document order, and an out-of-flow box is in the list of
/// the box that positions it rather than of the one that declared it, which is where it is painted.
/// At each box the children are sorted into Appendix E's passes, then by `z-index`, then by
/// [`tree_rank`]. That is the order they are laid out in, which `order` on a flex item has already
/// moved, exactly as it moves painting, with each out-of-flow box put back where it was written.
pub fn paint_order(store: &LayoutStore, root: BoxKey) -> Vec<BoxKey> {
    let mut order = Vec::new();
    push_subtree(store, root, &mut order);
    order
}

/// Appends one box and everything below it, in painting order.
fn push_subtree(store: &LayoutStore, key: BoxKey, order: &mut Vec<BoxKey>) {
    order.push(key);
    for &child in children_in_paint_order(store, key).iter() {
        push_subtree(store, child, order);
    }
}

/// Whether one box's children are painted in the order they are laid out.
///
/// True of nearly every box: a container of ordinary block or inline children puts all of them in
/// one pass at one index. A container holding a positioned child, a float or a `z-index` paints
/// them in another order, and anything placing a newborn fragment among its siblings by their
/// layout position has to know that.
///
/// The box remembers a "yes" until a write can change it, so the fragment diff can ask on each
/// visit.
pub fn children_in_layout_order(store: &LayoutStore, key: BoxKey) -> bool {
    let Some(node) = store.get(key) else {
        return true;
    };
    if node.layout_order.known() {
        return true;
    }
    // One child is in order with itself, and its key is the costly part.
    if node.children.len() < 2 {
        return true;
    }
    let mut previous = None;
    let mut kept = true;
    for (position, &child) in node.children.iter().enumerate() {
        let rank = paint_key(store, key, node, position, child);
        if previous.is_some_and(|last| last > rank) {
            return false;
        }
        previous = Some(rank);
        kept &= reads_only_itself(store, key, child);
    }
    if kept {
        node.layout_order.remember();
    }
    true
}

/// Whether the paint key of `child`, a layout child of `parent`, reads only the child's own record.
///
/// The store clears the parent's memo on each write to such a record, so only these keys can be
/// remembered. The tree rank of a re-parented box reads other boxes as well.
fn reads_only_itself(store: &LayoutStore, parent: BoxKey, child: BoxKey) -> bool {
    store.get(child).is_some_and(|node| {
        node.parent == Some(parent)
            && !(matches!(
                node.style.get_box().position,
                PositionValue::Absolute | PositionValue::Fixed
            ) && store.declared(child).is_some())
    })
}

/// What a box sorts by among its siblings that its style decides: pass, `z-index`, and whether
/// its tree rank is looked up.
///
/// The store compares it across a style change to know whether the parent's memo still holds.
pub(crate) fn sort_inputs(node: &BoxNode) -> (u8, i32, bool) {
    (
        level_of(node) as u8,
        z_of(node),
        matches!(
            node.style.get_box().position,
            PositionValue::Absolute | PositionValue::Fixed
        ),
    )
}

/// One box's children, in the order they are painted.
///
/// Sorted by [`PaintKey`]: two children in the same pass with the same `z-index` paint in tree
/// order, which is the tie-break the specification gives.
///
/// Borrowed where the two orders already agree, which is nearly every box; recognising it costs one
/// walk and saves two allocations and the sort, per box, per pass.
pub fn children_in_paint_order(store: &LayoutStore, key: BoxKey) -> Cow<'_, [BoxKey]> {
    let Some(node) = store.get(key) else {
        return Cow::Borrowed(&[]);
    };
    if children_in_layout_order(store, key) {
        return Cow::Borrowed(&node.children);
    }
    let mut children: Vec<(PaintKey, BoxKey)> = node
        .children
        .iter()
        .enumerate()
        .map(|(position, &child)| (paint_key(store, key, node, position, child), child))
        .collect();
    children.sort_by_key(|(rank, _)| *rank);
    Cow::Owned(children.into_iter().map(|(_, child)| child).collect())
}

#[cfg(test)]
mod tests {
    use zgui_arena::DocumentId;
    use zgui_css::values::size::{PositionValue, ZIndexValue};
    use zgui_css::{ComputedStyle, StyleDraft};
    use zgui_dom::side::BoxKey;

    use super::{PaintLevel, children_in_layout_order, children_in_paint_order};
    use crate::boxtree::absolute::Declared;
    use crate::node::box_node::BoxNode;
    use crate::node::kind::{BoxKind, FormattingContext};
    use crate::tree::store::LayoutStore;

    /// A block box with `children` block children, all laid out and painted in tree order.
    fn container(children: usize) -> (LayoutStore, BoxKey, Vec<BoxKey>) {
        let mut store = LayoutStore::new(DocumentId::FIRST);
        let block = |store: &mut LayoutStore| {
            let mut node = BoxNode::new(
                StyleDraft::initial().build(),
                BoxKind::Element,
                FormattingContext::Block,
            );
            node.block_level = true;
            store.insert(node)
        };
        let root = block(&mut store);
        let kids: Vec<BoxKey> = (0..children).map(|_| block(&mut store)).collect();
        for &kid in &kids {
            store.get_mut(kid).expect("live").parent = Some(root);
        }
        store.get_mut(root).expect("live").children = kids.clone();
        (store, root, kids)
    }

    fn indexed(z: i32) -> ComputedStyle {
        let mut draft = StyleDraft::initial();
        draft.box_group().position = PositionValue::Relative;
        draft.position_group().z_index = ZIndexValue::Integer(z);
        draft.build()
    }

    fn remembered(store: &LayoutStore, key: BoxKey) -> bool {
        store.node(key).layout_order.known()
    }

    #[test]
    fn a_z_index_change_clears_the_remembered_layout_order() {
        let (mut store, root, kids) = container(3);
        assert!(children_in_layout_order(&store, root));
        assert!(remembered(&store, root), "the answer was not remembered");

        // The first child now paints after its siblings.
        assert!(store.set_style(kids[0], &indexed(1)));
        assert!(!remembered(&store, root));
        assert!(!children_in_layout_order(&store, root));
        assert_eq!(
            &*children_in_paint_order(&store, root),
            &[kids[1], kids[2], kids[0]]
        );

        // Back to a level that sorts in tree order again.
        assert!(store.set_style(kids[0], &indexed(0)));
        assert!(store.set_style(kids[1], &indexed(0)));
        assert!(store.set_style(kids[2], &indexed(0)));
        assert!(children_in_layout_order(&store, root));
        assert!(remembered(&store, root));
        assert!(store.set_style(kids[2], &indexed(-1)));
        assert!(!children_in_layout_order(&store, root));
    }

    #[test]
    fn a_restyle_that_keeps_the_sort_keeps_the_remembered_layout_order() {
        let (mut store, root, kids) = container(2);
        assert!(children_in_layout_order(&store, root));
        let larger = StyleDraft::initial()
            .with_font_size(zgui_geom::CssPx(32.0))
            .build();
        assert!(store.set_style(kids[1], &larger));
        assert!(
            remembered(&store, root),
            "a font size change forgot the order of the parent's children"
        );
    }

    #[test]
    fn a_child_list_write_clears_the_remembered_layout_order() {
        let (mut store, root, kids) = container(2);
        store.set_style(kids[0], &indexed(1));
        store.set_style(kids[1], &indexed(2));
        assert!(children_in_layout_order(&store, root));

        store.get_mut(root).expect("live").children.reverse();
        assert!(!remembered(&store, root));
        assert!(!children_in_layout_order(&store, root));

        // A write to a child clears its parent's memo too.
        store.get_mut(root).expect("live").children.reverse();
        assert!(children_in_layout_order(&store, root));
        store.get_mut(kids[1]).expect("live").block_level = false;
        assert!(!remembered(&store, root));
    }

    #[test]
    fn a_removed_child_clears_the_remembered_layout_order() {
        let (mut store, root, kids) = container(2);
        assert!(children_in_layout_order(&store, root));
        store.remove(kids[1]);
        assert!(!remembered(&store, root));
    }

    #[test]
    fn an_out_of_flow_record_keeps_the_layout_order_unremembered() {
        let (mut store, root, kids) = container(2);
        let mut draft = StyleDraft::initial();
        draft.box_group().position = PositionValue::Absolute;
        assert!(store.set_style(kids[1], &draft.build()));

        // An absolute child without a record sorts by its own record alone.
        assert!(children_in_layout_order(&store, root));
        assert!(remembered(&store, root));

        // Its record names where it was written, so its key reads other boxes from now on.
        store.set_declared(
            kids[1],
            Some(Declared {
                parent: root,
                after: Some(kids[0]),
            }),
        );
        assert!(!remembered(&store, root), "a new record kept the memo");
        assert!(children_in_layout_order(&store, root));
        assert!(
            !remembered(&store, root),
            "an answer that reads the out-of-flow record was remembered"
        );
    }

    #[test]
    fn a_copied_box_holds_no_remembered_layout_order() {
        let (store, root, _) = container(2);
        assert!(children_in_layout_order(&store, root));
        assert!(!store.node(root).clone().layout_order.known());
    }

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
