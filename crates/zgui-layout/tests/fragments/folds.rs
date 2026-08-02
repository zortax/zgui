//! What an unwind folds up the tree: the read-extent registry, blending, and disjointness.

use zgui_bits::DamageSet;
use zgui_layout::FragmentFlags;
use zgui_layout::fragment::diff::Everything;

use crate::probe::{box_named, own_fragment};
use crate::support::{Element, Fixture, Frame, fragments, lay_out, measurer, relayout};

#[test]
fn a_blurred_fragment_is_in_the_read_extent_registry_and_leaves_it_when_the_blur_goes() {
    let mut fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("panel").classes(&["blur"])]),
        "root { display: block; width: 200px }
         panel { display: block; height: 50px }
         .blur { filter: blur(4px) }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 200.0, 200.0);

    let panel = box_named(&store, &fixture, "panel");
    let fragment = own_fragment(&store, panel);
    assert!(fragment.flags.contains(FragmentFlags::HAS_READ_EXTENT));
    assert_eq!(store.read_extents(), &[fragment.key]);
    assert!(
        fragment.ink.size.height.0 > 50.0,
        "a blur paints outside the border box"
    );

    let target = fixture
        .document
        .store()
        .core(fixture.root)
        .first_child()
        .expect("the panel");
    fixture.edit_and_restyle(|edit| {
        edit.remove_class(target, zgui_interned::ClassName::new("blur"));
    });
    let mut store = fixture.box_tree();
    let mut content = measurer();
    relayout(&mut frame, &mut store, &mut content, 200.0, 200.0);
    let panel = box_named(&store, &fixture, "panel");
    assert!(
        !own_fragment(&store, panel)
            .flags
            .contains(FragmentFlags::HAS_READ_EXTENT)
    );
    assert!(store.read_extents().is_empty());
}

#[test]
fn a_blending_descendant_is_folded_all_the_way_up() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("wrap").children(vec![Element::new("blend")]),
        ]),
        "root { display: block; width: 200px }
         wrap { display: block }
         blend { display: block; height: 20px; mix-blend-mode: multiply }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 200.0, 200.0);

    for name in ["root", "wrap", "blend"] {
        let key = box_named(&store, &fixture, name);
        assert!(
            own_fragment(&store, key)
                .flags
                .contains(FragmentFlags::HAS_BLENDING_DESCENDANT),
            "`{name}` did not learn about the blend below it"
        );
    }
}

#[test]
fn overlapping_children_make_a_subtree_not_disjoint() {
    // The fold is `own ink disjoint from every child's, the children's pairwise disjoint, and every
    // child disjoint in itself`. The middle clause is the one that discriminates here, so the
    // fixture gives the parent no area of its own: a container whose own ink covers its children —
    // which is nearly every container — is reported not-disjoint whatever the children do, and that
    // answer is conservative in the safe direction. It costs a composited group that was not
    // strictly needed, and never a wrong pixel.
    let css = |second_top: &str| {
        format!(
            "root {{ display: block; width: 200px }}
             wrap {{ display: block; height: 0; position: relative }}
             a {{ display: block; height: 40px; width: 40px; position: absolute; top: 0 }}
             b {{ display: block; height: 40px; width: 40px; position: absolute; top: {second_top} }}"
        )
    };
    let disjointness = |second_top: &str| {
        let fixture = Fixture::new(
            Element::new("root").children(vec![
                Element::new("wrap").children(vec![Element::new("a"), Element::new("b")]),
            ]),
            &css(second_top),
        );
        let mut store = fixture.box_tree();
        let mut content = measurer();
        lay_out(&mut store, &mut content, 200.0, 200.0);
        let wrap = box_named(&store, &fixture, "wrap");
        own_fragment(&store, wrap).subtree_disjoint
    };
    assert!(
        disjointness("60px"),
        "40 tall from 0 and from 60 do not meet"
    );
    assert!(
        !disjointness("10px"),
        "40 tall from 0 and from 10 overlap by thirty pixels"
    );
}

#[test]
fn subtree_disjoint_is_independent_of_the_damage_set() {
    // The whole reason this field is decided here, over ink, is that it must not depend on what a
    // frame happened to paint. Two passes over the same tree — one whose damage starts empty and
    // one that starts covering the surface — have to agree on every fragment.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("a"),
            Element::new("b"),
            Element::new("c").children(vec![Element::new("d")]),
        ]),
        "root { display: block; width: 200px; position: relative }
         a { display: block; height: 40px }
         b { display: block; height: 40px; position: absolute; top: 20px }
         c { display: block; height: 40px }
         d { display: block; height: 10px }",
    );

    let answers = |damage: DamageSet| {
        let mut store = fixture.box_tree();
        let mut content = measurer();
        {
            let mut tree = zgui_layout::tree::LayoutTree::new(
                &mut store,
                &mut content,
                zgui_layout::DeviceStyle::default(),
            );
            assert!(tree.layout_root(taffy::Size {
                width: 200.0,
                height: 200.0
            }));
        }
        let mut frame = Frame::new();
        frame.damage = damage;
        let root = store.root().expect("a root");
        fragments(&mut frame, &mut store, root, &mut Everything);
        let mut answers = Vec::new();
        for key in store.keys() {
            for frag in store.fragments_of_box(key) {
                let fragment = store.fragment(*frag).expect("live");
                answers.push((key.index(), fragment.subtree_disjoint));
            }
        }
        answers.sort_unstable();
        answers
    };

    let partial = answers(DamageSet::new());
    let full = answers(DamageSet::full());
    assert!(!partial.is_empty());
    assert_eq!(partial, full);
}

#[test]
fn a_box_that_draws_lines_over_itself_is_not_disjoint() {
    // The pieces a box is painted as are not one thing: the box's own piece draws its background
    // and each line of its text is a piece of its own, drawn on top. In the fragment tree those
    // lines hang below the box's own piece, so the fold has to test them against it — folding them
    // into one union first makes every paragraph in every document report that nothing in it
    // overlaps anything, which is exactly the answer that lets an `opacity` be folded into paint
    // alpha and double-blend the text over its own background.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("para").text("alpha bravo delta gamma kappa sigma omega tau iota"),
        ]),
        "root { display: block; width: 120px }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 120.0, 400.0);

    let holder = store
        .keys()
        .into_iter()
        .find(|key| store.fragments_of_box(*key).len() > 1)
        .expect("some box was painted as more than one piece");
    let pieces = store.fragments_of_box(holder).to_vec();
    let own = store.fragment(pieces[0]).expect("live").ink;
    assert!(
        pieces[1..].iter().any(|line| {
            let ink = store.fragment(*line).expect("live").ink;
            !ink.is_empty() && own.intersects(ink)
        }),
        "the fixture stopped drawing its lines inside its own piece"
    );
    assert!(
        !store.fragment(pieces[0]).expect("live").subtree_disjoint,
        "a piece the box draws over itself is an overlap in its subtree"
    );
}
