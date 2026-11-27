//! The boxes the layout store marks as having children that may paint in another order.

use zgui_arena::DocumentId;
use zgui_css::values::size::{PositionValue, ZIndexValue};
use zgui_css::{ComputedStyle, StyleDraft};
use zgui_dom::side::BoxKey;
use zgui_layout::node::box_node::BoxNode;
use zgui_layout::node::kind::{BoxKind, FormattingContext};
use zgui_layout::tree::store::LayoutStore;
use zgui_space::ChangeCoverage;

/// A block box with three block children.
fn container() -> (LayoutStore, BoxKey, Vec<BoxKey>) {
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
    let kids: Vec<BoxKey> = (0..3).map(|_| block(&mut store)).collect();
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

fn relative() -> ComputedStyle {
    let mut draft = StyleDraft::initial();
    draft.box_group().position = PositionValue::Relative;
    draft.build()
}

fn isolated() -> ComputedStyle {
    let mut draft = StyleDraft::initial();
    draft.box_group().position = PositionValue::Relative;
    draft.box_group().isolation = zgui_css::values::effect::IsolationValue::Isolate;
    draft.build()
}

/// The boxes marked since `version`, each once.
fn marked(
    store: &LayoutStore,
    version: zgui_layout::JournalVersion,
) -> (ChangeCoverage, Vec<BoxKey>) {
    let mut marked = Vec::new();
    let coverage = store.order_dirty_since(version, &mut marked);
    marked.sort_unstable();
    marked.dedup();
    (coverage, marked)
}

#[test]
fn a_z_index_change_marks_the_parent() {
    let (mut store, root, kids) = container();
    let version = store.order_version();

    store.set_style(kids[1], &indexed(4));

    let (coverage, marked) = marked(&store, version);
    assert_eq!(coverage, ChangeCoverage::Delta);
    assert!(marked.contains(&root), "{marked:?}");
}

#[test]
fn a_restyle_that_keeps_the_sort_marks_nothing() {
    let (mut store, _, kids) = container();
    let version = store.order_version();

    let larger = StyleDraft::initial()
        .with_font_size(zgui_geom::CssPx(32.0))
        .build();
    store.set_style(kids[1], &larger);

    assert_eq!(marked(&store, version), (ChangeCoverage::Delta, vec![]));
}

#[test]
fn starting_a_stacking_context_marks_the_box_and_its_parent() {
    let (mut store, root, kids) = container();
    // Positioned already, so it sorts in the same pass whether it isolates or not.
    store.set_style(kids[2], &relative());
    let version = store.order_version();

    store.set_style(kids[2], &isolated());

    let (_, marked) = marked(&store, version);
    assert!(
        marked.contains(&root) && marked.contains(&kids[2]),
        "{marked:?}"
    );
}

#[test]
fn a_child_list_write_marks_the_box() {
    let (mut store, root, _) = container();
    let version = store.order_version();

    store.get_mut(root).expect("live").children.reverse();

    assert!(marked(&store, version).1.contains(&root));
}

#[test]
fn a_version_of_another_store_asks_for_everything() {
    let (first, _, _) = container();
    let (second, _, _) = container();

    assert_eq!(
        marked(&second, first.order_version()).0,
        ChangeCoverage::All
    );
}
