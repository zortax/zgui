//! That no engine style struct is ever built, and that a ten-thousand-box layout runs anyway.
//!
//! One convenience conversion in one style getter would make every box materialise an engine style
//! per frame, and nothing about the resulting layout would look wrong — it would simply cost more
//! than the layout it feeds. The property is therefore checked two ways: the type the getters
//! return cannot be an owned style, and no source file in the crate constructs one.

mod support;

use std::path::{Path, PathBuf};

use taffy::LayoutPartialTree;
use zgui_arena::DocumentId;
use zgui_css::StyleDraft;
use zgui_layout::measure::NoContent;
use zgui_layout::node::box_node::BoxNode;
use zgui_layout::node::kind::{BoxKind, FormattingContext};
use zgui_layout::style::{DeviceStyle, StyleRef};
use zgui_layout::tree::LayoutTree;
use zgui_layout::tree::store::LayoutStore;

/// How many boxes the fixture holds.
const BOXES: usize = 10_000;

/// How tall each of the children is, in device pixels.
const CHILD_HEIGHT: f32 = 7.0;

/// A flat block container with `BOXES - 1` block-level children, each of a fixed height.
///
/// The height is what makes the layout assertable: children with no height of their own all stack
/// at the same origin, and every "was it laid out" assertion over them is satisfied by a pass that
/// stopped after the first child.
fn fixture() -> LayoutStore {
    let mut store = LayoutStore::new(DocumentId::FIRST);
    let root_style = StyleDraft::initial().build();
    let style = {
        let mut draft = StyleDraft::initial();
        draft.position_group().height = zgui_css::values::size::SizeValue::LengthPercentage(
            zgui_css::values::length::NonNegative(
                zgui_css::values::length::LengthPercentage::new_length(
                    zgui_css::values::length::Length::new(CHILD_HEIGHT),
                ),
            ),
        );
        draft.build()
    };
    let root = store.insert(BoxNode::new(
        root_style,
        BoxKind::Element,
        FormattingContext::Block,
    ));
    store.get_mut(root).expect("live").block_level = true;
    let mut children = Vec::with_capacity(BOXES - 1);
    for _ in 0..BOXES - 1 {
        let child = store.insert(BoxNode::new(
            style.clone(),
            BoxKind::Element,
            FormattingContext::Block,
        ));
        let node = store.get_mut(child).expect("live");
        node.block_level = true;
        node.parent = Some(root);
        children.push(child);
    }
    let node = store.get_mut(root).expect("live");
    node.children = children.clone();
    node.paint_children = children;
    store.set_root(root);
    store
}

#[test]
fn a_ten_thousand_box_layout_runs_and_places_every_box() {
    let mut store = fixture();
    let mut content = NoContent;
    {
        let mut tree = LayoutTree::new(&mut store, &mut content, DeviceStyle::default());
        assert!(tree.layout_root(taffy::Size {
            width: 1000.0,
            height: 800.0
        }));
    }
    let root = store.root().expect("a root");
    assert_eq!(store.node(root).children.len(), BOXES - 1);
    // Every box was laid out, and each sits one child-height below the one before it. A layout that
    // stopped early — or one that never placed anything — leaves the tail at the origin, which is a
    // position a non-negativity check cannot tell apart from a real one.
    for (index, &child) in store.node(root).children.iter().enumerate() {
        let layout = store.layout_of(child).expect("laid out");
        #[expect(clippy::cast_precision_loss, reason = "ten thousand is exact in f32")]
        let expected = index as f32 * CHILD_HEIGHT;
        assert_eq!(layout.origin.y.0, expected, "child {index} sits wrong");
        assert_eq!(layout.size.height.0, CHILD_HEIGHT);
        assert_eq!(layout.size.width.0, 1000.0, "child {index} did not stretch");
    }
    assert_eq!(
        store.layout_of(root).expect("laid out").size.width.0,
        1000.0
    );
}

#[test]
fn the_style_the_algorithms_read_is_a_borrow_and_not_an_owned_style() {
    // The type system is what makes this hold rather than discipline: the associated type the
    // container-style getter returns is a borrow, so a getter *cannot* return a built style.
    fn assert_borrowed<'a, T>()
    where
        T: LayoutPartialTree,
        for<'b> T::CoreContainerStyle<'b>: Copy,
    {
    }
    assert_borrowed::<LayoutTree<'_, NoContent>>();

    assert!(
        size_of::<StyleRef<'_>>() < size_of::<taffy::Style<zgui_interned::Ident>>(),
        "the borrowed view is not smaller than the style it replaces"
    );
    // And smaller by a wide margin, not by a field: a view that grew to the size of a style would
    // be a style with extra steps.
    assert!(size_of::<StyleRef<'_>>() <= 64);
}

#[test]
fn no_source_file_in_this_crate_builds_an_engine_style() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = Vec::new();
    collect(&root, &mut sources);
    assert!(
        sources.len() > 20,
        "found only {} source files, so the scan covers nothing",
        sources.len()
    );

    let mut offenders = Vec::new();
    let mut saw_taffy = false;
    for path in &sources {
        let text = std::fs::read_to_string(path).expect("a readable source file");
        if text.contains("taffy::") {
            saw_taffy = true;
        }
        if text.contains("taffy::Style") {
            offenders.push(format!("{} names `taffy::Style`", path.display()));
        }
        // A bare `Style` would have to be imported to be constructible, and an import from the
        // layout engine is what that looks like.
        for line in text.lines().filter(|line| line.contains("use taffy::")) {
            if line
                .split(|character: char| !character.is_alphanumeric() && character != '_')
                .any(|word| word == "Style")
            {
                offenders.push(format!("{} imports the engine's own style", path.display()));
            }
        }
    }
    // The control: if nothing in the crate mentioned the layout engine at all, the scan above
    // would pass while reading the wrong files.
    assert!(saw_taffy, "no source file mentions the layout engine");
    assert!(offenders.is_empty(), "{offenders:#?}");
}

/// Every `.rs` file below `directory`.
fn collect(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a readable directory") {
        let path = entry.expect("a readable entry").path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}
