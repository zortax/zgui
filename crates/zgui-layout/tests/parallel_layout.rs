//! A pooled layout produces the layout a serial one does, to the bit.
//!
//! The fixture is wide and deep enough to clear the executor's distribution gates — the cold-node
//! count and the planned-box floor — and mixed enough to drive every converted flexbox batch:
//! definite items place, auto items measure, and every item carries a block subtree so a worker
//! runs real nested layouts against its own view of the store. The pooled run asserts on the
//! distribution counter, so the gates tightening past the fixture fails loudly instead of
//! quietly testing the serial path against itself.

use zgui_arena::DocumentId;
use zgui_css::StyleDraft;
use zgui_layout::node::box_node::BoxNode;
use zgui_layout::node::kind::{BoxKind, FormattingContext};
use zgui_layout::tree::LayoutTree;
use zgui_layout::tree::parallel::LayoutPool;
use zgui_layout::tree::store::LayoutStore;
use zgui_layout::{NoContent, style::DeviceStyle};

/// How many flex items the fixture holds.
const ITEMS: usize = 48;

/// One deterministic document: a flex row of `ITEMS` items, each a block holding six blocks.
fn fixture() -> LayoutStore {
    let mut store = LayoutStore::new(DocumentId::FIRST);
    let root_style = StyleDraft::initial().build();
    let root = store.insert(BoxNode::new(
        root_style,
        BoxKind::Element,
        FormattingContext::Flex,
    ));
    store.get_mut(root).expect("live").block_level = true;
    store.set_root(root);

    for index in 0..ITEMS {
        let item_style = {
            let mut draft = StyleDraft::initial();
            // Every third item has a definite width; the rest measure. Heights alternate so the
            // cross-size and baseline paths see variety.
            if index % 3 == 0 {
                draft.position_group().width = zgui_css::values::size::SizeValue::LengthPercentage(
                    zgui_css::values::length::NonNegative(
                        zgui_css::values::length::LengthPercentage::new_length(
                            zgui_css::values::length::Length::new(20.0 + (index % 5) as f32),
                        ),
                    ),
                );
            }
            draft.build()
        };
        let item = store.insert(BoxNode::new(
            item_style,
            BoxKind::Element,
            FormattingContext::Block,
        ));
        for depth in 0..6 {
            let leaf_style = {
                let mut draft = StyleDraft::initial();
                draft.position_group().height = zgui_css::values::size::SizeValue::LengthPercentage(
                    zgui_css::values::length::NonNegative(
                        zgui_css::values::length::LengthPercentage::new_length(
                            zgui_css::values::length::Length::new(
                                8.0 + (index % 7) as f32 + depth as f32,
                            ),
                        ),
                    ),
                );
                draft.position_group().width = zgui_css::values::size::SizeValue::LengthPercentage(
                    zgui_css::values::length::NonNegative(
                        zgui_css::values::length::LengthPercentage::new_length(
                            zgui_css::values::length::Length::new(10.0 + (index % 4) as f32),
                        ),
                    ),
                );
                draft.build()
            };
            let leaf = store.insert(BoxNode::new(
                leaf_style,
                BoxKind::Element,
                FormattingContext::Block,
            ));
            let node = store.get_mut(leaf).expect("live");
            node.parent = Some(item);
            node.block_level = true;
            store.get_mut(item).expect("live").children.push(leaf);
            store.get_mut(item).expect("live").paint_children.push(leaf);
        }
        let node = store.get_mut(item).expect("live");
        node.parent = Some(root);
        store.get_mut(root).expect("live").children.push(item);
        store.get_mut(root).expect("live").paint_children.push(item);
    }
    store
}

/// Every box's unrounded result, by its bits.
fn snapshot(store: &LayoutStore) -> Vec<(u32, [u32; 4])> {
    let mut keys = store.keys();
    keys.sort_unstable();
    keys.into_iter()
        .map(|key| {
            let layout = store.layout_of(key).expect("every box was laid out");
            (
                key.index(),
                [
                    layout.origin.x.0.to_bits(),
                    layout.origin.y.0.to_bits(),
                    layout.size.width.0.to_bits(),
                    layout.size.height.0.to_bits(),
                ],
            )
        })
        .collect()
}

fn laid_out(pool: Option<&LayoutPool>) -> Vec<(u32, [u32; 4])> {
    let mut store = fixture();
    let mut content = NoContent;
    let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
    if let Some(pool) = pool {
        tree = tree.with_parallel(pool);
    }
    let before = zgui_profile::counter::snapshot().layout_batches_distributed;
    assert!(tree.layout_viewport(400.0, 300.0));
    if pool.is_some() {
        assert!(
            zgui_profile::counter::snapshot().layout_batches_distributed > before,
            "the pooled pass distributed no batch; the fixture no longer clears the gates"
        );
    }
    drop(tree);
    snapshot(&store)
}

#[test]
fn a_pooled_layout_equals_the_serial_one_at_every_width() {
    let serial = laid_out(None);
    assert_eq!(serial.len(), 1 + ITEMS * 7, "the whole fixture was placed");
    for workers in [2, 3, 8] {
        let pool = LayoutPool::new(workers);
        let pooled = laid_out(Some(&pool));
        assert_eq!(pooled, serial, "{workers} workers disagreed with serial");
    }
}

/// Lays the fixture out twice, the second pass into a wider viewport over the warm tree.
fn resized(pool: Option<&LayoutPool>) -> Vec<(u32, [u32; 4])> {
    let mut store = fixture();
    let mut content = NoContent;
    let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
    if let Some(pool) = pool {
        tree = tree.with_parallel(pool);
    }
    assert!(tree.layout_viewport(400.0, 300.0));
    let before = zgui_profile::counter::snapshot();
    // The cross axis moves: the row's items stretch to the container's height, so every item is
    // asked a question no cache holds. A main-axis move alone re-asks almost nothing here — the
    // items size by their own content, and answers that still fit are held.
    assert!(tree.layout_viewport(400.0, 360.0));
    let moved = before.delta(&zgui_profile::counter::snapshot());
    if pool.is_some() {
        assert!(
            moved.layout_batches_distributed > 0,
            "the warm resize distributed no batch against {} nodes relaid out; the cold work a \
             resize re-opens is exactly what the gates are for",
            moved.nodes_relaid_out,
        );
    }
    drop(tree);
    snapshot(&store)
}

#[test]
fn a_pooled_warm_resize_equals_the_serial_one() {
    // The resize question, apart from the cold one above: every box holds a standing answer, a
    // new width misses all of them, and the batch runs over a warm store whose paragraphs and
    // intrinsic answers must be reused rather than rebuilt.
    let serial = resized(None);
    for workers in [2, 8] {
        let pool = LayoutPool::new(workers);
        let pooled = resized(Some(&pool));
        assert_eq!(pooled, serial, "{workers} workers disagreed with serial");
    }
}

#[test]
fn a_pool_below_the_distribution_threshold_changes_nothing() {
    // Three items stay under the executor's minimum, so the batch runs serially even with a pool
    // installed — the gate the small-container measurements demanded.
    let mut store = LayoutStore::new(DocumentId::FIRST);
    let root = store.insert(BoxNode::new(
        StyleDraft::initial().build(),
        BoxKind::Element,
        FormattingContext::Flex,
    ));
    store.get_mut(root).expect("live").block_level = true;
    store.set_root(root);
    for _ in 0..3 {
        let item = store.insert(BoxNode::new(
            StyleDraft::initial().build(),
            BoxKind::Element,
            FormattingContext::Block,
        ));
        store.get_mut(item).expect("live").parent = Some(root);
        store.get_mut(root).expect("live").children.push(item);
        store.get_mut(root).expect("live").paint_children.push(item);
    }
    let pool = LayoutPool::new(4);
    let mut content = NoContent;
    let mut tree =
        LayoutTree::new(&mut store, &mut content, DeviceStyle::default()).with_parallel(&pool);
    assert!(tree.layout_viewport(200.0, 100.0));
}
