//! What the paint stage has to get right, asserted against a real cascade and real fragments.

mod support;

use support::{Element, Harness};
use zgui_bits::DamageSet;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_layout::fragment::diff::pixels;

/// A root with `count` identically classed children.
fn rows(count: usize) -> Element {
    Element::new("root").children(
        (0..count)
            .map(|_| Element::new("li").classes(&["btn"]))
            .collect(),
    )
}

/// A rectangle in device pixels.
fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect<i32, Device> {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

#[test]
fn a_clean_subtree_far_from_the_damage_is_skipped_whole() {
    let mut harness = Harness::sized(
        rows(200),
        "root { display: block; width: 300px }
         .btn { display: block; height: 20px; background: #eee }",
        300.0,
        4000.0,
    );
    let everything = harness.paint_everything();

    // A second frame with one row's worth of damage at the very top.
    harness.clear_damage();
    harness.damage.absorb(rect(0, 0, 300, 20));
    let report = harness.paint();
    assert!(
        report.skipped_subtrees > 0,
        "nothing was skipped whole, so the constant-time subtree test never fired"
    );
    assert!(
        report.emitted.len() * 10 < everything.emitted.len(),
        "one damaged row emitted {} of the document's {} fragments",
        report.emitted.len(),
        everything.emitted.len()
    );
}

#[test]
fn every_fragment_the_damage_reaches_is_emitted() {
    // The emission-completeness oracle. It is the direct test of where the expansion runs: a
    // damage-rectangle assertion structurally cannot see a region that was added and then painted
    // by nobody.
    let mut harness = Harness::new(
        Element::new("root").children(vec![
            Element::new("panel").children(vec![Element::new("inner")]),
            Element::new("aside"),
        ]),
        "root { display: block; width: 300px }
         panel { display: block; height: 60px; background: #eee }
         inner { display: block; height: 20px; background: #333 }
         aside { display: block; height: 40px; background: #999 }",
    );
    harness.paint_everything();

    harness.clear_damage();
    harness.damage.absorb(rect(0, 0, 300, 30));
    harness.expand();
    let report = harness.paint();
    report.assert_emission_complete(&harness.store, &harness.damage);
}

#[test]
fn a_blurred_panel_over_animating_content_has_its_source_region_repainted() {
    // The case the expansion exists for, and the case an expansion folded into the emit walk
    // misses: the blurred panel's own pixels are untouched, so its whole ancestor chain misses the
    // damage and the constant-time subtree skip drops it.
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("under"), Element::new("dialog")]),
        "root { display: block; width: 300px; height: 400px }
         under { display: block; height: 200px; background: #eee }
         dialog { display: block; height: 60px; backdrop-filter: blur(10px) }",
        300.0,
        400.0,
    );
    harness.paint_everything();
    assert!(
        !harness.store.read_extents().is_empty(),
        "the fixture has to register a read extent, or this measures nothing"
    );

    // Only the content *under* the dialog changed.
    harness.clear_damage();
    harness.damage.absorb(rect(0, 190, 300, 20));
    let before = harness.damage.rects().to_vec();
    let expansion = harness.expand();
    assert!(
        expansion.absorbed > 0,
        "the dialog's source region was never absorbed"
    );
    assert_ne!(
        harness.damage.rects(),
        before.as_slice(),
        "the damage set did not grow"
    );

    let dialog = harness.fragment_of("dialog");
    let source =
        zgui_paint::read_extent_of(&harness.store, dialog, harness.scale).expect("a read extent");
    assert!(
        harness.damage.is_full()
            || harness
                .damage
                .rects()
                .iter()
                .any(|rect| rect.contains_rect(pixels(source.source))),
        "one rectangle has to contain the whole source region, or the composite samples across two \
         separate passes"
    );

    let report = harness.paint();
    report.assert_emission_complete(&harness.store, &harness.damage);
}

#[test]
fn a_per_pixel_backdrop_is_not_expanded_for() {
    // The corollary, and it is worth asserting because expanding for it would cost a great deal and
    // buy nothing: the pixels it reads are the pixels it is already covering.
    let mut harness = Harness::new(
        Element::new("root").children(vec![Element::new("header")]),
        "root { display: block; width: 300px; height: 300px }
         header { display: block; height: 40px; backdrop-filter: saturate(180%) }",
    );
    harness.paint_everything();
    harness.clear_damage();
    harness.damage.absorb(rect(0, 0, 300, 40));
    let expansion = harness.expand();
    assert_eq!(expansion.absorbed, 0);
    assert!(!expansion.escalated);
}

#[test]
fn a_full_window_blur_escalates_to_a_full_redraw() {
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("fog")]),
        "root { display: block; width: 200px; height: 200px }
         fog { display: block; height: 200px; filter: blur(60px) }",
        200.0,
        200.0,
    );
    harness.paint_everything();
    harness.clear_damage();
    harness.damage.absorb(rect(90, 90, 20, 20));
    let expansion = harness.expand();
    assert!(
        expansion.escalated,
        "a region past half the surface is cheaper as one full redraw"
    );
    assert!(harness.damage.is_full());
}

#[test]
fn a_removed_subtree_damages_the_area_it_occupied() {
    // The one rectangle no living fragment can report: what compares output between frames only
    // ever sees output that still exists.
    let mut harness = Harness::new(
        Element::new("root").children(vec![Element::new("keep"), Element::new("gone")]),
        "root { display: block; width: 300px }
         keep { display: block; height: 40px; background: #eee }
         gone { display: block; height: 60px; background: #333 }",
    );
    harness.paint_everything();

    let key = harness.document.store().key_of(harness.element("gone"));
    let vacated_ink = zgui_layout::fragment::index::ink_of(&harness.store, key);
    assert!(
        !vacated_ink.is_empty(),
        "the fixture's removed box has to have painted something"
    );

    let gone = harness.element("gone");
    harness.edit_and_restyle(|batch| batch.remove(gone));
    harness.clear_damage();

    let absorbed = zgui_paint::vacated(&mut harness.document, &harness.store, &mut harness.damage);
    assert_eq!(absorbed, 1, "one root was removed and one contributed");
    assert!(
        harness
            .damage
            .rects()
            .iter()
            .any(|rect| rect.contains_rect(pixels(vacated_ink))),
        "the area the removed panel occupied is not in the damage: {:?}",
        harness.damage.rects()
    );

    // The list is consumed rather than borrowed, which is why there is exactly one consumer and
    // this is it: a second reader would find it emptied and absorb nothing, silently.
    let again = zgui_paint::vacated(&mut harness.document, &harness.store, &mut harness.damage);
    assert_eq!(again, 0);
}

#[test]
fn every_group_marker_is_matched() {
    // Half a pair leaves a target open, or composites one that was never begun — and neither shows
    // up as anything but a wrong pixel much later.
    let mut harness = Harness::new(
        Element::new("root").children(vec![
            Element::new("card").children(vec![Element::new("inner")]),
        ]),
        "root { display: block; width: 300px }
         card { display: block; height: 60px; filter: blur(4px) }
         inner { display: block; height: 20px; background: #333 }",
    );
    harness.paint_everything();
    let starts = harness
        .scene
        .primitives
        .groups
        .iter()
        .filter(|marker| marker.is_start)
        .count();
    let ends = harness.scene.primitives.groups.len() - starts;
    assert!(
        starts > 0,
        "the fixture has to open a group, or this counts nothing"
    );
    assert_eq!(starts, ends, "every group opened has to be closed");
}

#[test]
fn an_outline_is_drawn_over_the_boxs_own_descendants() {
    // Appendix E step ten. A box's outline sorts above its children's content, and a stage that
    // emitted it beside the background would put a focus ring underneath the label it rings.
    let mut harness = Harness::new(
        Element::new("root").children(vec![
            Element::new("field").children(vec![Element::new("label")]),
        ]),
        "root { display: block; width: 300px }
         field { display: block; height: 40px; outline: 2px solid #06f; background: #fff }
         label { display: block; height: 20px; background: #333 }",
    );
    harness.paint_everything();

    let quads = &harness.scene.primitives.quads;
    let outlines: Vec<_> = quads.iter().filter(|quad| quad.border[0] == 2.0).collect();
    assert_eq!(outlines.len(), 1, "one outline, from {} quads", quads.len());
    let outline_order = outlines[0].order;
    let label = quads
        .iter()
        .find(|quad| quad.bounds[3] == 20.0)
        .expect("the label's own quad");
    assert!(
        outline_order > label.order,
        "the outline at {outline_order} sorts under the label at {}",
        label.order
    );
}

#[test]
fn nothing_outside_the_damage_reaches_the_display_list() {
    let mut harness = Harness::sized(
        rows(100),
        "root { display: block; width: 300px }
         .btn { display: block; height: 20px; background: #eee }",
        300.0,
        2000.0,
    );
    harness.paint_everything();
    let everything = harness.scene.primitives.quads.len();

    harness.clear_damage();
    harness.damage.absorb(rect(0, 0, 300, 40));
    harness.paint();
    let damaged = harness.scene.primitives.quads.len();
    assert!(
        damaged * 4 < everything,
        "a two-row damage emitted {damaged} of {everything} quads"
    );
    assert!(
        damaged > 0,
        "it emitted nothing at all, which is a black frame"
    );
}

#[test]
fn a_damaged_frame_emits_a_subset_of_what_a_full_one_does() {
    // The half of a damage-correctness comparison a display list can state. The other half needs
    // pixels; this one fails outright if the damage test ever admits something a full frame does
    // not draw.
    let mut harness = Harness::new(
        Element::new("root").children(vec![Element::new("a"), Element::new("b")]),
        "root { display: block; width: 300px }
         a { display: block; height: 40px; background: #eee }
         b { display: block; height: 40px; background: #333 }",
    );
    let full = harness.paint_everything();
    let everything: Vec<_> = full.emitted.clone();
    assert!(everything.len() >= 3, "root, a and b");

    harness.clear_damage();
    harness.damage.absorb(rect(0, 0, 300, 20));
    let partial = harness.paint();
    assert!(!partial.emitted.is_empty());
    for frag in &partial.emitted {
        assert!(
            everything.contains(frag),
            "the damaged frame emitted {frag:?}, which the full frame did not"
        );
    }
}

#[test]
fn an_empty_damage_set_paints_nothing_and_a_full_one_paints_everything() {
    let mut harness = Harness::new(
        Element::new("root").children(vec![Element::new("a")]),
        "root { display: block; width: 300px }
         a { display: block; height: 40px; background: #eee }",
    );
    harness.damage = DamageSet::new();
    let none = harness.paint();
    assert_eq!(none.primitives, 0);

    let all = harness.paint_everything();
    assert!(all.primitives > 0);
}

#[test]
fn the_expansion_settles_over_two_overlapping_blurred_panels() {
    // Two blurred panels whose source regions overlap: growing the damage to cover the first can
    // bring the second into range, which is why the loop runs to a fixpoint rather than once.
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("one"), Element::new("two")]),
        "root { display: block; width: 400px; height: 400px }
         one { display: block; height: 40px; filter: blur(6px) }
         two { display: block; height: 40px; filter: blur(6px) }",
        400.0,
        400.0,
    );
    harness.paint_everything();
    harness.clear_damage();
    harness.damage.absorb(rect(0, 0, 10, 10));
    let expansion = harness.expand();
    assert!(expansion.passes >= 1);
    assert!(
        !expansion.escalated,
        "two small panels are not a full redraw"
    );
    assert!(expansion.absorbed >= 1);
}

#[test]
fn a_replayed_ranges_indices_still_resolve_to_what_they_were_recorded_with() {
    // A replayed range carries *last* frame's clip, paint and transform indices. They resolve only
    // because the side tables are kept across frames and an entry keeps its identity while anything
    // refers to it — a table rebuilt per frame would draw one fragment with another's paint, with no
    // error anywhere. The debug assertion inside the cache is what catches that; this is the fixture
    // that runs it, over content that actually carries a clip and a transform.
    let mut harness = Harness::sized(
        Element::new("root").children(vec![
            Element::new("port").children(vec![Element::new("row")]),
        ]),
        "root { display: block; width: 300px; height: 300px }
         port { display: block; height: 100px; overflow: hidden; transform: translateX(4px) }
         row { display: block; height: 40px; background: #eee }",
        300.0,
        300.0,
    );
    harness.paint_everything();
    let row = harness.fragment_of("row");
    let clip = harness.store.fragment(row).expect("a fragment").clip;
    assert_ne!(
        clip,
        zgui_scene::ClipId::ROOT,
        "the row has to be clipped, or the invariant has nothing to check"
    );
    let recorded = harness.scene.clips.content_hash(clip);
    assert!(recorded.is_some());

    // A second frame replays it, and the assertion inside the cache runs against these indices.
    let report = harness.paint_everything();
    assert!(report.emitted.contains(&row));
    assert_eq!(
        harness.scene.clips.content_hash(clip),
        recorded,
        "the chain the replayed range names has to resolve to the content it was recorded with"
    );
}

#[test]
fn the_emit_walk_and_the_hit_index_agree_about_painting_order() {
    // A hit is the last thing painted under the point, so the sequence the hit index is built from
    // and the sequence the display list is emitted in are one order and not two. They are computed
    // by different walks — one flat, one with an inside and an outside — and this is what stops the
    // two from drifting apart in a way that shows up only as a click landing on the wrong element.
    let mut harness = Harness::sized(
        Element::new("root").children(vec![
            Element::new("under").classes(&["back"]),
            Element::new("mid").children(vec![Element::new("relative")]),
            Element::new("over").classes(&["front"]),
            Element::new("floated"),
        ]),
        "root { display: block; width: 300px; height: 400px; position: relative; z-index: 0 }
         under, mid, over, floated { display: block; height: 40px; background: #eee }
         .back { position: relative; z-index: -1 }
         .front { position: relative; z-index: 3 }
         relative { display: block; height: 20px; position: relative; background: #333 }
         floated { float: left; width: 40px }",
        300.0,
        400.0,
    );
    let report = harness.paint_everything();

    let root = harness.store.root().expect("a root box");
    let expected: Vec<_> = zgui_layout::fragment::stacking::paint_order(&harness.store, root)
        .into_iter()
        .filter(|key| !harness.store.fragments_of_box(*key).is_empty())
        .collect();
    let mut painted: Vec<_> = Vec::new();
    for frag in &report.emitted {
        let box_ = harness.store.fragment(*frag).expect("a live fragment").box_;
        if painted.last() != Some(&box_) {
            painted.push(box_);
        }
    }
    assert!(
        expected.len() >= 5,
        "the fixture has to have depth and passes"
    );
    assert_eq!(
        painted, expected,
        "the walk that emits and the walk that indexes hits put the boxes in different orders"
    );
}

#[test]
fn emit_and_hit_agree_on_the_same_matrix() {
    // Which matrix a hit is answered against, asserted across the two stages that have to agree
    // about it. A transition ticks by rewriting the matrix under a coordinate system's name; the
    // frame in front of the pointer was composed through whatever that name resolved to when it was
    // drawn, and a query has to answer against that one and not against the matrix the element
    // started from or the one it is going to.
    //
    // It is worth a test rather than a sentence because the two stages reach that matrix by
    // different routes — the emitter writes the name onto every primitive it pushes, the index
    // reads the name off the entry it filed — and nothing else in this suite would see them part
    // company. The fragment does not move, so every border box agrees; the primitive is drawn
    // through the same node, so the transcript agrees; and the pixels are right, so the damage
    // oracle agrees. The element is drawn animated and hit-tested un-animated, and only this fails.
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("card")]),
        "root { display: block; width: 300px; height: 300px }
         card { display: block; width: 40px; height: 40px; background: #333;
                transform: translateX(100px) }",
        300.0,
        300.0,
    );
    harness.paint_everything();

    let frag = harness.fragment_of("card");
    let card = harness
        .store
        .fragment(frag)
        .expect("a live fragment")
        .border_box;
    // The name the *emitter* wrote onto what it pushed, not the one the fragment carries: that they
    // are the same name is half of what is under test.
    let slot = harness
        .scene
        .primitives
        .quads
        .iter()
        .find(|quad| quad.transform != zgui_scene::SpatialId::VIEWPORT.index())
        .expect("the card is drawn under a coordinate system of its own")
        .transform;
    let space = harness
        .scene
        .spatial
        .at(slot)
        .expect("the slot a primitive names is a live coordinate system");

    let answers = |harness: &Harness, x: f32| {
        harness
            .hit
            .hit(
                zgui_geom::Point::new(zgui_geom::DevicePx(x), zgui_geom::DevicePx(20.0)),
                &harness.scene.clips,
                &harness.scene.spatial,
            )
            .contains(&frag)
    };
    let drawn = |harness: &Harness| {
        let matrix = harness
            .scene
            .spatial
            .resolve(space)
            .expect("a live coordinate system");
        zgui_layout::fragment::transform::transformed_bounds(&matrix, card)
    };

    assert_eq!(drawn(&harness).origin.x.0, 100.0);
    assert!(answers(&harness, 120.0), "hit where the frame drew it");
    assert!(!answers(&harness, 20.0), "and not where layout put it");

    // One tick of a transition: the matrix under the name is rewritten, and *nothing else happens*.
    // No fragment is recomposed, no hit entry is rewritten, no primitive is emitted again. This is
    // the frame C2 will produce, and the whole reason hit entries had to stop naming the device.
    let owner = zgui_scene::PropertyOwner::of(harness.box_of("card"));
    let held = *harness
        .scene
        .spatial
        .get(space)
        .expect("a live coordinate system");
    let moved = harness.scene.spatial.establish(
        owner,
        zgui_scene::SpatialNode {
            local: zgui_geom::Matrix4::translation(200.0, 0.0, 0.0),
            ..held
        },
    );
    assert_eq!(moved, space, "a tick moves a matrix and keeps the name");

    assert_eq!(drawn(&harness).origin.x.0, 200.0);
    assert!(answers(&harness, 220.0), "the hit follows the matrix");
    assert!(
        !answers(&harness, 120.0),
        "and stops answering where the previous frame drew it",
    );
}
