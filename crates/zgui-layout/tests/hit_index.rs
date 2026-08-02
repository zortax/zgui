//! What one animation tick costs the hit index.
//!
//! Its own target, and the only case in it, because the counters it reads are process-wide: a case
//! running beside it would move them and the budget would be measuring two things at once.

mod support;

use support::{Element, Fixture, fragments, lay_out, measurer};
use zgui_bits::Dirty;
use zgui_layout::fragment::diff::Everything;

#[test]
fn a_transform_transition_updates_only_its_own_subtree_in_the_hit_index() {
    // A transform tick marks one row and its subtree. The index must move exactly those entries
    // and rebuild nothing: at the plan's own thousand-row fixture a rebuild per tick would cost
    // more than the whole scroll budget.
    let rows: Vec<Element> = (0..40)
        .map(|_| Element::new("row").children(vec![Element::new("cell")]))
        .collect();
    let mut fixture = Fixture::new(
        Element::new("root").children(rows),
        "root { display: block; width: 300px }
         row { display: block; height: 20px }
         cell { display: block; height: 10px }
         .moved { transform: translateX(12px) }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 800.0);
    let indexed = frame.hit.len();
    assert!(indexed >= 81, "one piece per box, and there are many");

    let target = fixture
        .document
        .store()
        .core(fixture.root)
        .first_child()
        .expect("the first row");
    fixture.edit_and_restyle(|edit| {
        edit.add_class(target, zgui_interned::ClassName::new("moved"));
    });
    let mut store = fixture.box_tree();
    let mut content = measurer();
    {
        let mut tree = zgui_layout::tree::LayoutTree::new(
            &mut store,
            &mut content,
            zgui_layout::DeviceStyle::default(),
        );
        assert!(tree.layout_root(taffy::Size {
            width: 300.0,
            height: 800.0
        }));
    }
    let root = store.root().expect("a root");
    // The box tree was rebuilt, so the index starts from the whole document once; the tick that
    // follows is the case under test.
    fragments(&mut frame, &mut store, root, &mut Everything);

    // The first row, which is the one the class was added to: `box_named` finds *a* row, and a
    // budget measured on a row that was never transformed would be a budget for doing nothing.
    let row = store.node(root).children[0];
    let subtree: u64 = {
        let cell = store.node(row).children[0];
        (store.fragments_of_box(row).len() + store.fragments_of_box(cell).len()) as u64
    };

    let before = zgui_profile::counter::snapshot();
    let marked = [row, store.node(row).children[0]];
    let elements: Vec<zgui_dom::NodeKey> = marked
        .iter()
        .filter_map(|key| store.get(*key).and_then(|node| node.source))
        .collect();
    let ancestors: Vec<zgui_dom::NodeKey> = {
        let mut held = Vec::new();
        let mut cursor = Some(row);
        while let Some(key) = cursor {
            if let Some(source) = store.get(key).and_then(|node| node.source) {
                held.push(source);
            }
            cursor = store.get(key).and_then(|node| node.parent);
        }
        held
    };
    let mut dirty = OnlySubtree {
        elements,
        ancestors,
        bits: Dirty::REFRAGMENT | Dirty::REHIT,
    };
    let mut tables = zgui_layout::fragment::build::Tables {
        clips: &mut frame.clips,
        spatial: &mut frame.spatial,
        device: zgui_layout::DeviceStyle::default(),
        scroll: &frame.scroll,
        placements: &[],
    };
    zgui_layout::fragment::diff::rebuild(
        &mut store,
        &mut frame.hit,
        &mut tables,
        &mut dirty,
        root,
        &mut frame.damage,
    );
    let after = zgui_profile::counter::snapshot();
    let delta = before.delta(&after);

    assert_eq!(
        delta.hit_index_rebuilds, 0,
        "moving one row must not rebuild the index"
    );
    assert_eq!(
        delta.hit_entries_updated, subtree,
        "exactly the moved subtree's pieces were touched"
    );
    assert_eq!(
        delta.nodes_visited, 3,
        "the root, the marked row and its cell — every other row's subtree was left alone. A gate \
         that consulted what the *subtree* owes instead of what each child owes would compose all \
         forty rows to service one."
    );
    assert_eq!(
        frame.hit.spaces(),
        2,
        "the document's own coordinate system and the moved row's, and one hierarchy each. A query \
         pays per hierarchy, so a document that minted one for every box would make every pointer \
         move cost the document."
    );

    // And the entries really do describe the transformed row rather than the place layout put it —
    // but not by holding a rectangle that moved. An entry is in its own space, and the transform is
    // a property of that space, so the two rows' rectangles are identical and the *spaces* they
    // name are what differ. Resolving each is what puts the moved one twelve pixels along.
    let moved = frame
        .hit
        .entry(store.fragments_of_box(row)[0])
        .expect("the moved row is indexed");
    let still = frame
        .hit
        .entry(store.fragments_of_box(store.node(root).children[1])[0])
        .expect("a row that did not move is indexed");
    assert_eq!(
        moved.envelope.origin.x.0, still.envelope.origin.x.0,
        "a tick moves a matrix, and an entry that named the device would have had to be rewritten"
    );
    let placed = |entry: &zgui_layout::HitEntry| {
        let space = entry
            .space
            .expect("every fragment names a coordinate system");
        let matrix = frame
            .spatial
            .resolve(space)
            .expect("a live coordinate system");
        zgui_layout::fragment::transform::transformed_bounds(&matrix, entry.envelope)
    };
    assert_eq!(placed(moved).origin.x.0 - placed(still).origin.x.0, 12.0);
}

/// A dirty answer that marks one subtree and reports everything else clean.
///
/// This is what one tick of a transform transition looks like from the fragment pass's side: a
/// handful of nodes carry `REFRAGMENT` and `REHIT`, and the rest of the document carries nothing.
struct OnlySubtree {
    /// The elements that owe work themselves.
    elements: Vec<zgui_dom::NodeKey>,
    /// The elements that have to be descended through to reach them.
    ancestors: Vec<zgui_dom::NodeKey>,
    /// What they owe.
    bits: Dirty,
}

impl zgui_layout::fragment::diff::FrameDirty for OnlySubtree {
    fn own(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        match node {
            Some(node) if self.elements.contains(&node) => self.bits,
            _ => Dirty::empty(),
        }
    }

    fn subtree(&self, node: Option<zgui_dom::NodeKey>) -> Dirty {
        match node {
            // A box with no element of its own has no marks to read, so it is never dismissed.
            None => self.bits,
            Some(node) if self.ancestors.contains(&node) => self.bits,
            Some(_) => Dirty::empty(),
        }
    }

    fn mark(&mut self, _node: Option<zgui_dom::NodeKey>, _bits: Dirty) {}

    fn retire(&mut self, _node: Option<zgui_dom::NodeKey>, _phase: Dirty) {
        // The marks are this fixture's own two sets, not a document's, and each case runs one
        // pass: there is nothing a retirement could write back to.
    }
}
