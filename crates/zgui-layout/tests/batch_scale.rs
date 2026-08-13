//! How distribution scales with document width, measured rather than assumed.
//!
//! Under a debug build the probe runs one small round, which checks that a pooled cold pass
//! completes on a distributable document and prints nothing worth reading. The numbers come
//! from a release run:
//!
//! ```text
//! cargo test -p zgui-layout --release --test batch_scale -- --nocapture
//! ```
//!
//! `ZGUI_SCALE_ITEMS`, `ZGUI_SCALE_DEPTH` and `ZGUI_SCALE_ROUNDS` change the fixture; the
//! release defaults model a wide dashboard of cards, which is the shape the executor
//! distributes and the shipped bench documents lack. The 256-box distribution floor in
//! `tree/executor.rs` was set from this probe's output.

use std::time::Instant;

use zgui_arena::DocumentId;
use zgui_css::StyleDraft;
use zgui_layout::node::box_node::BoxNode;
use zgui_layout::node::kind::{BoxKind, FormattingContext};
use zgui_layout::tree::LayoutTree;
use zgui_layout::tree::parallel::LayoutPool;
use zgui_layout::tree::store::LayoutStore;
use zgui_layout::{NoContent, style::DeviceStyle};

/// One deterministic document: a flex row of `items` cards, each a chain of `depth` blocks.
fn fixture(items: usize, depth: usize) -> LayoutStore {
    let mut store = LayoutStore::new(DocumentId::FIRST);
    let root = store.insert(BoxNode::new(
        StyleDraft::initial().build(),
        BoxKind::Element,
        FormattingContext::Flex,
    ));
    store.get_mut(root).expect("live").block_level = true;
    store.set_root(root);

    for index in 0..items {
        let item = store.insert(BoxNode::new(
            StyleDraft::initial().build(),
            BoxKind::Element,
            FormattingContext::Block,
        ));
        let mut parent = item;
        for level in 0..depth {
            let leaf_style = {
                let mut draft = StyleDraft::initial();
                draft.position_group().height = zgui_css::values::size::SizeValue::LengthPercentage(
                    zgui_css::values::length::NonNegative(
                        zgui_css::values::length::LengthPercentage::new_length(
                            zgui_css::values::length::Length::new(
                                4.0 + (index % 7) as f32 + (level % 3) as f32,
                            ),
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
            node.parent = Some(parent);
            node.block_level = true;
            store.get_mut(parent).expect("live").children.push(leaf);
            store
                .get_mut(parent)
                .expect("live")
                .paint_children
                .push(leaf);
            parent = leaf;
        }
        let node = store.get_mut(item).expect("live");
        node.parent = Some(root);
        store.get_mut(root).expect("live").children.push(item);
        store.get_mut(root).expect("live").paint_children.push(item);
    }
    store
}

/// One cold pass over a fresh store, in microseconds.
fn cold_pass(items: usize, depth: usize, pool: Option<&LayoutPool>) -> f64 {
    let mut store = fixture(items, depth);
    let mut content = NoContent;
    let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
    if let Some(pool) = pool {
        tree = tree.with_parallel(pool);
    }
    let start = Instant::now();
    assert!(tree.layout_viewport(1200.0, 800.0));
    start.elapsed().as_secs_f64() * 1e6
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[test]
fn distribution_against_width() {
    // A debug build measures nothing meaningful, so it runs the smallest fixture once and is
    // done: the probe still exercises a distributed cold pass on every plain test run.
    let debug = cfg!(debug_assertions);
    let depth = env_usize("ZGUI_SCALE_DEPTH", 20);
    let rounds = env_usize("ZGUI_SCALE_ROUNDS", if debug { 1 } else { 7 });
    let widths: &[usize] = if debug {
        &[24]
    } else {
        &[24, 64, 128, 256, 512]
    };
    let pool = LayoutPool::new(8);
    for &items in widths {
        let items = env_usize("ZGUI_SCALE_ITEMS", items);
        let mut serial: Vec<f64> = Vec::new();
        let mut pooled: Vec<f64> = Vec::new();
        for _ in 0..rounds {
            serial.push(cold_pass(items, depth, None));
            pooled.push(cold_pass(items, depth, Some(&pool)));
        }
        serial.sort_by(f64::total_cmp);
        pooled.sort_by(f64::total_cmp);
        let (serial, pooled) = (serial[rounds / 2], pooled[rounds / 2]);
        println!(
            "items={items:4} boxes={:6} serial={serial:9.1}us pooled={pooled:9.1}us ratio={:.2}",
            1 + items * (depth + 1),
            pooled / serial,
        );
        if std::env::var_os("ZGUI_SCALE_ITEMS").is_some() {
            break;
        }
    }
}
