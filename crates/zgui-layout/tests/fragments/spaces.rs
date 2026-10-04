//! Which coordinate system a fragment names, across a pass that changes it.

use zgui_bits::Dirty;
use zgui_dom::NodeKey;
use zgui_layout::fragment::diff::FrameDirty;

use crate::probe::{box_named, own_fragment};
use crate::support::{Element, Fixture, fragments, lay_out, measurer};

/// One element owes a repaint, and nothing else in the document owes anything.
struct Repainted(Option<NodeKey>);

impl FrameDirty for Repainted {
    fn own(&self, node: Option<NodeKey>) -> Dirty {
        if node.is_some() && node == self.0 {
            Dirty::REPAINT
        } else {
            Dirty::empty()
        }
    }

    fn subtree(&self, node: Option<NodeKey>) -> Dirty {
        self.own(node)
    }

    fn mark(&mut self, _node: Option<NodeKey>, _bits: Dirty) {}

    fn retire(&mut self, _node: Option<NodeKey>, _phase: Dirty) {}
}

/// A card with a row and a cell below it, under `card`'s declarations.
fn card(declarations: &str) -> Fixture {
    Fixture::new(
        Element::new("root").children(vec![Element::new("card").children(vec![
            Element::new("row").children(vec![Element::new("cell")]),
        ])]),
        &format!(
            "root {{ display: block; width: 200px }}
             card {{ display: block; {declarations} }}
             row {{ display: block; height: 20px }}
             cell {{ display: block; height: 10px }}"
        ),
    )
}

#[test]
fn a_box_that_gives_its_coordinate_system_back_hands_its_clean_children_the_one_above() {
    // An identity transform establishes a coordinate system and moves nothing, so the card's
    // matrix is the identity before and after the transform goes. The row and the cell owe
    // nothing and did not move. Only the name of the system they are drawn in changes.
    let fixture = card("transform: translate(0px, 0px)");
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let root = box_named(&store, &fixture, "root");
    let held = box_named(&store, &fixture, "card");
    let above = own_fragment(&store, root).transform;
    assert_ne!(
        own_fragment(&store, held).transform,
        above,
        "the transformed card draws in a system of its own"
    );

    let flat = card("");
    let flat_store = flat.box_tree();
    let style = flat_store
        .node(box_named(&flat_store, &flat, "card"))
        .style
        .clone();
    store.set_style(held, &style);
    let mut dirty = Repainted(store.node(held).source);
    fragments(&mut frame, &mut store, root, &mut dirty);
    // The frame boundary, where a system given back stops resolving.
    frame.spatial.recycle();

    assert_eq!(own_fragment(&store, held).transform, above);
    for name in ["row", "cell"] {
        let fragment = own_fragment(&store, box_named(&store, &fixture, name));
        assert_eq!(
            fragment.transform, above,
            "`{name}` names the system the card gave back"
        );
        assert!(
            fragment
                .transform
                .is_some_and(|space| frame.spatial.contains(space)),
            "`{name}` names a system that no longer resolves"
        );
    }
}

#[test]
fn a_box_that_takes_a_coordinate_system_hands_it_to_its_clean_children() {
    // The scale moves the card's matrix and leaves the row and the cell where they are in it. A
    // pass that only carried them onto the new matrix would leave them naming the system above
    // the card, which draws them without its scale.
    let fixture = card("");
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let root = box_named(&store, &fixture, "root");
    let held = box_named(&store, &fixture, "card");
    let scaled = card("transform: scale(2)");
    let scaled_store = scaled.box_tree();
    let style = scaled_store
        .node(box_named(&scaled_store, &scaled, "card"))
        .style
        .clone();
    store.set_style(held, &style);
    let mut dirty = Repainted(store.node(held).source);
    fragments(&mut frame, &mut store, root, &mut dirty);

    let own = own_fragment(&store, held).transform;
    assert_ne!(own, own_fragment(&store, root).transform);
    for name in ["row", "cell"] {
        let fragment = own_fragment(&store, box_named(&store, &fixture, name));
        assert_eq!(
            fragment.transform, own,
            "`{name}` is drawn outside the card's scale"
        );
    }
}
