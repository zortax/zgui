//! The values a fragment keeps outside its record: where the record states them and where the store
//! holds them.

use zgui_layout::fragment::identity_hash;

use crate::hits::Clean;
use crate::probe::{box_named, own_fragment};
use crate::support::{Element, Fixture, fragments, lay_out, measurer};

/// A plain box, a clipping box with a child, and a turned box with a child.
fn fixture() -> Fixture {
    Fixture::new(
        Element::new("root").children(vec![
            Element::new("plain"),
            Element::new("clipper").children(vec![Element::new("clipped")]),
            Element::new("turned").children(vec![Element::new("inside")]),
        ]),
        "root { display: block; width: 200px }
         plain { display: block; height: 20px }
         clipper { display: block; height: 20px; overflow: hidden }
         clipped { display: block; height: 40px }
         turned { display: block; height: 20px; transform: rotate(10deg) }
         inside { display: block; height: 10px }",
    )
}

#[test]
fn a_plain_or_clipped_box_states_its_extras_in_its_record() {
    let fixture = fixture();
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let _frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let plain = own_fragment(&store, box_named(&store, &fixture, "plain"));
    let extra = store.fragment_extra(plain);
    assert_eq!(extra.clip_transform, None, "nothing clips it");
    assert_eq!(extra.transform_hash, identity_hash());
    assert_eq!(extra.content_hash, 0);

    let clipped = own_fragment(&store, box_named(&store, &fixture, "clipped"));
    assert!(!clipped.clip.is_root());
    assert_eq!(
        store.fragment_extra(clipped).clip_transform,
        clipped.transform,
        "its clip is measured in the space it is drawn in"
    );
}

#[test]
fn a_turned_subtree_keeps_its_fingerprints_in_the_store_until_it_is_recycled() {
    let fixture = fixture();
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let turned = box_named(&store, &fixture, "turned");
    let inside = box_named(&store, &fixture, "inside");
    let turned_hash = store
        .fragment_extra(own_fragment(&store, turned))
        .transform_hash;
    assert_ne!(turned_hash, identity_hash());
    assert_eq!(
        store
            .fragment_extra(own_fragment(&store, inside))
            .transform_hash,
        turned_hash,
        "a child is composed under the same matrix"
    );
    assert_eq!(
        store.spilled_fragment_count(),
        2,
        "only the turned box and its child spill"
    );

    let root = store.root().expect("a root");
    let gone = own_fragment(&store, turned).key;
    zgui_layout::boxtree::patch::detach(&mut store, turned);
    {
        let mut tree = zgui_layout::tree::LayoutTree::new(
            &mut store,
            &mut content,
            zgui_layout::DeviceStyle::default(),
        );
        assert!(tree.layout_root(cephal::Size {
            width: 200.0,
            height: 200.0
        }));
    }
    fragments(&mut frame, &mut store, root, &mut Clean);
    assert_eq!(
        store
            .fragment_extra_of(gone)
            .map(|extra| extra.transform_hash),
        Some(turned_hash),
        "a destroyed fragment reads as it was until the frame is recycled"
    );
    store.recycle();
    assert_eq!(store.fragment_extra_of(gone), None);
    assert_eq!(store.spilled_fragment_count(), 0);
}
