//! What a layout pass may be spared, and what it may never be spared.
//!
//! Three savings are asserted here, and each of them is a way to make the engine do less: a pass
//! that is not run at all, a measurement that is not taken again, and a subtree the fragment pass
//! does not walk into. Every one of them is a way to leave the wrong pixels on the screen, so every
//! case pairs the counter that proves the saving with a comparison against the answer the same
//! document produces when nothing is skipped.

mod support;

use std::sync::{Mutex, MutexGuard, PoisonError};

use support::{Content, Element, Fixture, Frame, lay_out_only, measurer};
use zgui_dom::{NodeIndex, NodeKind};
use zgui_layout::tree::gate::Relayout;
use zgui_layout::tree::store::LayoutStore;
use zgui_layout::tree::{LayoutTree, print};
use zgui_profile::{Counter, counter};

/// The counter block is process-wide, so cases that write to it take turns.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The viewport every case in this file lays out into.
const VIEWPORT: (f32, f32) = (600.0, 400.0);

/// Runs a gated pass and reports what it decided.
fn gated(store: &mut LayoutStore, content: &mut Content, width: f32, height: f32) -> Relayout {
    let mut tree = LayoutTree::new(store, content, zgui_layout::style::DeviceStyle::default());
    tree.relayout_root(taffy::Size { width, height })
}

/// How many boxes were laid out and how many measurements were served while `body` ran.
fn cost(body: impl FnOnce()) -> (u64, u64) {
    counter::reset();
    body();
    (
        counter::get(Counter::NodesRelaidOut),
        counter::get(Counter::SizesHeld),
    )
}

/// A three-column grid of panels, each holding a heading and a paragraph.
///
/// The shape that makes the difference visible: grid track sizing measures every item at
/// min-content and again at max-content, once per pass of an algorithm that runs several, so one
/// panel is asked the same handful of questions many times over.
fn panels(count: usize) -> Element {
    let mut children = Vec::new();
    for _ in 0..count {
        children.push(Element::new("panel").children(vec![
            Element::new("h").text("a heading of some length"),
            Element::new("row").children(vec![
                    Element::new("cell").children(vec![
                        Element::new("p")
                            .text("a paragraph with rather more words in it than the heading"),
                    ]),
                    Element::new("cell").children(vec![Element::new("p").text("a shorter one")]),
                ]),
        ]));
    }
    Element::new("root").children(children)
}

/// The sheet the panels are laid out by.
const SHEET: &str = "root { display: grid; grid-template-columns: auto auto auto; width: 600px }
     panel { display: block }
     h { display: block; font-size: 20px }
     row { display: flex; flex-direction: row }
     cell { display: flex; flex-direction: column; flex-grow: 1 }
     p { display: block; font-size: 12px }";

/// A fixture, its store laid out once, and the measurer that answered for its text.
fn settled(count: usize) -> (Fixture, LayoutStore, Content) {
    let fixture = Fixture::new(panels(count), SHEET);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out_only(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1);
    (fixture, store, content)
}

// -- the pass that is not run ------------------------------------------------------------------

/// A document nothing has invalidated is not laid out again, and the geometry it keeps is the
/// geometry a pass would have produced.
#[test]
fn a_settled_document_is_held_rather_than_laid_out_again() {
    let _guard = exclusive();
    let (_fixture, mut store, mut content) = settled(6);
    let settled_text = print::to_text(&store);

    let (relaid_out, _) = cost(|| {
        assert_eq!(
            gated(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1),
            Relayout::Held
        );
    });
    assert_eq!(relaid_out, 0, "a held pass laid a box out");
    assert_eq!(
        print::to_text(&store),
        settled_text,
        "holding the pass changed the geometry it was holding"
    );

    // And the answer it held is the answer running it would have produced. Run it, ungated, over
    // the very same store: every number must come out where it already was.
    lay_out_only(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1);
    assert_eq!(
        print::to_text(&store),
        settled_text,
        "the held geometry is not what an ungated pass produces for the same document"
    );
}

/// A viewport that moved is laid out for, even though no box was invalidated.
///
/// Nothing below the root can see the viewport, so no box is marked when it changes and the marks
/// alone would hold a document laid out for the previous window size for ever.
#[test]
fn a_viewport_that_moved_is_laid_out_for() {
    let _guard = exclusive();
    let (_fixture, mut store, mut content) = settled(6);
    assert_eq!(
        gated(&mut store, &mut content, 500.0, VIEWPORT.1),
        Relayout::Ran
    );
    let narrower = print::to_text(&store);

    // The same document built afresh and laid out into the narrower viewport, which is the answer
    // with no incremental path involved at all.
    let fixture = Fixture::new(panels(6), SHEET);
    let mut fresh = fixture.box_tree();
    let mut fresh_content = measurer();
    lay_out_only(&mut fresh, &mut fresh_content, 500.0, VIEWPORT.1);
    assert_eq!(
        narrower,
        print::to_text(&fresh),
        "the document laid out for a viewport it was gated into disagrees with a fresh one"
    );

    // And going back to the viewport it was settled at is a pass too, not a hold.
    assert_eq!(
        gated(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1),
        Relayout::Ran
    );
}

/// A scale change is laid out for, even into a viewport of the same number of device pixels.
#[test]
fn a_scale_change_is_laid_out_for() {
    let _guard = exclusive();
    let (_fixture, mut store, mut content) = settled(4);
    assert_eq!(
        gated(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1),
        Relayout::Held
    );
    zgui_layout::tree::dirty::mark_all_dirty(&mut store);
    assert_eq!(
        gated(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1),
        Relayout::Ran,
        "the document was held at a scale it was not laid out at"
    );
}

/// One box invalidated is enough to run the pass, and the pass agrees with a fresh one.
#[test]
fn one_invalidated_box_is_laid_out_for() {
    let _guard = exclusive();
    let (_fixture, mut store, mut content) = settled(6);
    let leaf = store
        .keys()
        .into_iter()
        .find(|&key| store.node(key).children.is_empty())
        .expect("the fixture has a leaf");
    zgui_layout::tree::dirty::mark_dirty(&mut store, leaf);
    assert_eq!(
        gated(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1),
        Relayout::Ran
    );

    let fixture = Fixture::new(panels(6), SHEET);
    let mut fresh = fixture.box_tree();
    let mut fresh_content = measurer();
    lay_out_only(&mut fresh, &mut fresh_content, VIEWPORT.0, VIEWPORT.1);
    assert_eq!(
        print::to_text(&store),
        print::to_text(&fresh),
        "the incremental pass over an invalidated box disagrees with a fresh layout"
    );
}

/// A document with no boxes at all is neither held nor laid out, and says so.
#[test]
fn a_document_with_no_boxes_is_reported_rather_than_held() {
    let _guard = exclusive();
    let mut store = LayoutStore::new(zgui_arena::DocumentId::FIRST);
    let mut content = measurer();
    assert_eq!(
        gated(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1),
        Relayout::NoRoot
    );
}

// -- the measurement that is not taken again -----------------------------------------------------

/// A grid re-measured after one leaf changed asks each panel for its size once rather than once
/// per track-sizing pass, and arrives at the geometry a fresh layout arrives at.
///
/// The engine's own per-box cache keeps nine slots chosen by the *shape* of the question, so the
/// min-content and max-content probes grid track sizing repeats against a moving grid-area estimate
/// all land in the same slot and evict each other. Each eviction is a whole nested layout of the
/// panel. This asserts that they are served instead, and — the half that matters — that serving
/// them changes no number anywhere.
#[test]
fn a_grid_serves_its_repeated_probes_rather_than_re_measuring() {
    let _guard = exclusive();
    let (_fixture, mut store, mut content) = settled(9);

    // A cold pass over the same document, for the two things it establishes: what the geometry is
    // when nothing at all is reused, and how many box layouts that costs.
    let fixture = Fixture::new(panels(9), SHEET);
    let mut cold = fixture.box_tree();
    let mut cold_content = measurer();
    let (cold_relaid_out, _) = cost(|| {
        lay_out_only(&mut cold, &mut cold_content, VIEWPORT.0, VIEWPORT.1);
    });
    let cold_text = print::to_text(&cold);
    assert_eq!(
        print::to_text(&store),
        cold_text,
        "the settled document and a cold one disagree before anything was invalidated"
    );

    // One leaf deep inside one panel is invalidated, which is what a keystroke does.
    let leaf = store
        .keys()
        .into_iter()
        .find(|&key| store.node(key).children.is_empty())
        .expect("the fixture has a leaf");
    zgui_layout::tree::dirty::mark_dirty(&mut store, leaf);

    let (warm_relaid_out, served) = cost(|| {
        assert_eq!(
            gated(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1),
            Relayout::Ran
        );
    });

    assert_eq!(
        print::to_text(&store),
        cold_text,
        "the pass that served its measurements from the memo produced different geometry from a \
         cold one"
    );
    assert!(served > 0, "no measurement was served from the memo at all");
    assert!(
        warm_relaid_out * 4 < cold_relaid_out,
        "a change to one leaf cost {warm_relaid_out} box layouts against a cold document's \
         {cold_relaid_out}, so the repeated probes are still being re-measured"
    );
}

/// A memo entry is never served for a box whose content changed under it.
///
/// The saving above is the whole reason a stale answer could be served, so this asserts the
/// invalidation directly: the same box, measured again once it holds different text, comes out at
/// the size the new text needs rather than at the size the old text had.
#[test]
fn a_box_whose_text_changed_is_measured_again() {
    let _guard = exclusive();
    const SHORT: &str = "ab";
    const LONG: &str = "abcdefghijklmnopqrstuvwx";
    const SHEET: &str =
        "root { display: block; width: 600px } p { display: inline-block; font-size: 10px }";

    let mut fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("p").text(SHORT)]),
        SHEET,
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out_only(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1);
    let narrow = print::to_text(&store);

    let text = first_text(&fixture.document, fixture.root);
    fixture.edit_and_restyle(|edit| edit.set_text(text, LONG));
    let root = fixture.document.root_index().expect("a root element");
    assert_eq!(
        zgui_layout::boxtree::patch::retext(&mut store, &fixture.document, root),
        zgui_layout::boxtree::patch::Retext::Patched(1),
        "the text was not rewritten in place, so this case is not testing the memo"
    );

    assert_eq!(
        gated(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1),
        Relayout::Ran
    );
    assert_ne!(
        print::to_text(&store),
        narrow,
        "the box holding the longer string was laid out at the size the shorter one had"
    );

    let fresh_fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("p").text(LONG)]),
        SHEET,
    );
    let mut fresh = fresh_fixture.box_tree();
    let mut fresh_content = measurer();
    lay_out_only(&mut fresh, &mut fresh_content, VIEWPORT.0, VIEWPORT.1);
    assert_eq!(
        print::to_text(&store),
        print::to_text(&fresh),
        "the re-measured document disagrees with one built from the same text"
    );
}

/// The first text node under `index`.
fn first_text(document: &zgui_dom::Document, index: NodeIndex) -> NodeIndex {
    if document.store().core(index).kind() == NodeKind::Text {
        return index;
    }
    let mut next = document.store().core(index).first_child();
    while let Some(child) = next {
        if let Some(found) = search(document, child) {
            return found;
        }
        next = document.store().core(child).next_sibling();
    }
    panic!("the fixture has no text node")
}

/// The same, answering nothing for a subtree with no text in it.
fn search(document: &zgui_dom::Document, index: NodeIndex) -> Option<NodeIndex> {
    if document.store().core(index).kind() == NodeKind::Text {
        return Some(index);
    }
    let mut next = document.store().core(index).first_child();
    while let Some(child) = next {
        if let Some(found) = search(document, child) {
            return Some(found);
        }
        next = document.store().core(child).next_sibling();
    }
    None
}

// -- the subtree the fragment pass does not walk into --------------------------------------------

/// An anonymous box does not stop its subtree being skipped, and the fragments that result are the
/// ones a pass that skipped nothing produces.
///
/// An anonymous box has no element and therefore no marks of its own. Asking about no element gets
/// "everything owed", for the box *and* its whole subtree — so one wrapper in the way makes every
/// box below it unskippable, and nearly a fifth of a real document's boxes are wrappers. The marks
/// come from the document itself, so this is the answer a frame gives rather than a stand-in.
#[test]
fn an_anonymous_box_does_not_block_its_subtree() {
    let _guard = exclusive();
    let mut fixture = Fixture::new(panels(9), SHEET);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = Frame::default();
    lay_out_only(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1);
    let root = store.root().expect("a root box");

    let boxes = store.keys().len() as u64;
    let wrappers = store
        .keys()
        .into_iter()
        .filter(|&key| store.node(key).source.is_none())
        .count();
    assert!(
        wrappers > 0,
        "the fixture has no anonymous boxes, so it tests nothing"
    );

    // The first pass composes everything and retires what it serviced, which is what leaves the
    // document with nothing owed.
    {
        let mut marks =
            zgui_layout::fragment::diff::DocumentMarks::for_document(&mut fixture.document);
        support::fragments(&mut frame, &mut store, root, &mut marks);
    }
    let composed = zgui_layout::tree::print::to_text(&store);

    // The second pass has nothing to do. It should reach the root, look at its children and stop.
    let visited = {
        counter::reset();
        let mut marks =
            zgui_layout::fragment::diff::DocumentMarks::for_document(&mut fixture.document);
        support::fragments(&mut frame, &mut store, root, &mut marks);
        counter::get(Counter::NodesVisited)
    };
    assert!(
        visited * 4 < boxes,
        "a settled pass visited {visited} of {boxes} boxes, so the anonymous wrappers are still \
         blocking their subtrees"
    );
    assert_eq!(
        zgui_layout::tree::print::to_text(&store),
        composed,
        "the pass that skipped subtrees left different geometry behind"
    );
}
