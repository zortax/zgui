//! A custom element that asks to be measured again is measured again.
//!
//! The implementation moves its layout revision and nothing else: no style changes, no box is
//! rebuilt, no text is rewritten. The only pass that can reach the box is the one for exactly
//! this, and a box it does not reach is laid out from an answer taken before the element changed.

mod support;

use support::{Element, Fixture, lay_out, measurer};
use zgui_dom::NodeIndex;
use zgui_layout::boxtree::patch::custom::relayout;
use zgui_layout::tree::dirty::is_dirty;
use zgui_vocab::prop::custom;
use zgui_vocab::{PropKey, PropValue};

/// Makes `node` a custom element with `token` at `layout_revision`.
fn own(fixture: &mut Fixture, node: NodeIndex, token: u32, layout_revision: u16) {
    fixture.edit_and_restyle(|edit| {
        edit.set_property(
            node,
            PropKey::new(custom::ELEMENT),
            Some(PropValue::Integer(custom::reference(
                token,
                layout_revision,
                0,
            ))),
        );
    });
}

#[test]
fn a_moved_layout_revision_throws_the_custom_box_s_layout_away() {
    let mut fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("meter")]),
        "root { display: block; width: 400px }
         meter { display: block; height: 20px }",
    );
    let root = fixture.root;
    let meter = fixture
        .document
        .store()
        .core(root)
        .first_child()
        .expect("the meter");
    own(&mut fixture, meter, 7, 0);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 400.0);
    // Ownership appearing owed a relayout too; a frame's fragment pass retires it. Here, retired
    // by hand, so that what follows measures the revision alone.
    zgui_dom::dirty::walk::walk(
        fixture.document.store_mut(),
        root,
        zgui_bits::Dirty::RELAYOUT,
        &mut |_, _| {},
    );

    let key = fixture.document.store().key_of(meter);
    let box_ = store.boxes_of(key)[0];
    assert!(!is_dirty(&store, box_), "laid out, and holding its answer");

    // Nothing moved: the pass leaves the answer alone.
    assert_eq!(relayout(&mut store, &fixture.document, root), 0);
    assert!(!is_dirty(&store, box_));

    // The implementation asked to be measured again.
    own(&mut fixture, meter, 7, 1);
    assert!(
        relayout(&mut store, &fixture.document, root) > 0,
        "the box and the path above it were invalidated"
    );
    assert!(is_dirty(&store, box_), "the held answer is gone");
}
