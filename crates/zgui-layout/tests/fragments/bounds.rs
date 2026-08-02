//! Where an element is, as the union of every piece it was painted as.

use zgui_layout::FragmentKind;

use crate::probe::{box_named, own_fragment};
use crate::support::{self, Element, Fixture, lay_out, measurer};

#[test]
fn an_elements_bounds_are_the_union_of_every_piece_it_was_painted_as() {
    // "Where is this element" is a union rather than a rectangle, because one element is painted
    // as several pieces. An element that generated no box at all answers with an empty rectangle,
    // which is a different thing from a zero-sized box and is told apart by asking whether it has
    // any pieces at all.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("para").text("alpha bravo delta gamma kappa sigma omega"),
            Element::new("gone"),
        ]),
        "root { display: block; width: 200px }
         para { display: block }
         gone { display: none }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 200.0, 400.0);

    let para = box_named(&store, &fixture, "para");
    let node = store.node(para).source.expect("an element");
    let pieces = store.fragments_of(node);
    assert!(!pieces.is_empty());
    let bounds = zgui_layout::fragment::index::bounds_of(&store, node);
    for piece in pieces {
        let fragment = store.fragment(*piece).expect("live");
        assert!(
            bounds.contains_rect(fragment.border_box),
            "a piece of the element sits outside its own bounds"
        );
    }
    assert!(
        zgui_layout::fragment::index::ink_of(&store, node).contains_rect(bounds),
        "everything below the element is inside the ink it takes with it"
    );

    let hidden = fixture.document.store().key_of(
        fixture
            .document
            .store()
            .core(fixture.root)
            .last_child()
            .expect("the hidden element"),
    );
    assert!(store.fragments_of(hidden).is_empty());
    assert!(
        zgui_layout::fragment::index::bounds_of(&store, hidden).is_empty(),
        "an element that generated nothing is nowhere"
    );
}

#[test]
fn a_replaced_box_says_what_it_draws() {
    let fixture = Fixture::with_natural_size(
        Element::new("root").children(vec![Element::new("picture").image(40.0, 30.0)]),
        "root { display: block; width: 200px }
         picture { display: block }",
        (40.0, 30.0),
    );
    let mut store = fixture.box_tree();
    let mut content = support::measurer_with_images(40.0, 30.0);
    lay_out(&mut store, &mut content, 200.0, 200.0);
    let picture = box_named(&store, &fixture, "picture");
    assert!(matches!(
        own_fragment(&store, picture).kind,
        FragmentKind::Replaced { .. }
    ));
}
