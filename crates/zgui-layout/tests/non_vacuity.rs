//! The proof that the layout engine's measurement memo can be used and can be refused.
//!
//! A box asked for its size twice with the same question can be answered from the first answer, and
//! the counter that records it is the one kind of counter that reads zero when the memo is working
//! perfectly, zero when it has stopped working, and zero when nobody ever incremented it. So a
//! bound written against it alone is green over an engine whose memo was deleted.
//!
//! What separates those cases is a pair of situations, and
//! [`assert_non_vacuous`](zgui_profile::counter::non_vacuity::assert_non_vacuous) drives both: one
//! the memo exists for, in which the counter must move, and one in which serving an answer would
//! give the wrong geometry, in which it must not. Both documents are laid out *inside* the
//! scenarios, because the counter block is taken for the length of the pair.

mod support;

use support::{Content, Element, Fixture, lay_out_only, measurer};
use zgui_dom::{Document, NodeIndex, NodeKind};
use zgui_layout::style::DeviceStyle;
use zgui_layout::tree::LayoutTree;
use zgui_layout::tree::store::LayoutStore;
use zgui_profile::Counter;
use zgui_profile::counter::non_vacuity::{Scenario, assert_non_vacuous};

/// The viewport both scenarios lay out into.
const VIEWPORT: (f32, f32) = (600.0, 400.0);

/// A three-column grid, which is the shape that asks one box the same question many times.
///
/// Track sizing measures every item at min-content and again at max-content, once per pass of an
/// algorithm that runs several against a moving grid-area estimate. The repeats are the memo's
/// whole reason to exist.
const GRID: &str = "root { display: grid; grid-template-columns: auto auto auto; width: 600px }
     panel { display: block }
     h { display: block; font-size: 20px }
     row { display: flex; flex-direction: row }
     cell { display: flex; flex-direction: column; flex-grow: 1 }
     p { display: block; font-size: 12px }";

/// One box holding text, which is measured once and then measured again when the text changes.
const ONE_BOX: &str = "root { display: block; width: 600px }
     p { display: inline-block; font-size: 10px }";

/// The text the box holds before the edit, and after it.
const SHORT: &str = "ab";
/// The wider string, whose size the box must not inherit from the narrower one.
const LONG: &str = "abcdefghijklmnopqrstuvwx";

#[test]
fn a_repeated_probe_is_served_and_a_box_whose_text_changed_is_not() {
    assert_non_vacuous(
        Counter::SizesHeld,
        Scenario::new(
            "a grid whose track sizing probes each panel repeatedly",
            grid,
        ),
        Scenario::new("a box measured again after its text changed", retexted),
    );
}

/// Lays out a grid of nine panels, cold.
fn grid() {
    let fixture = Fixture::new(panels(9), GRID);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out_only(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1);
}

/// Lays out one box, rewrites its text in place, and lays it out again.
///
/// The memo is keyed by the question rather than by the answer, so the box is asked the same
/// question either side of the edit; what must not happen is that the second asking is served from
/// the first. The size is asserted as well as the counter, because a memo that served the stale
/// answer and a memo that was never consulted are the same number and different pictures.
fn retexted() {
    let mut fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("p").text(SHORT)]),
        ONE_BOX,
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out_only(&mut store, &mut content, VIEWPORT.0, VIEWPORT.1);
    let narrow = zgui_layout::tree::print::to_text(&store);

    let text = first_text(&fixture.document, fixture.root);
    fixture.edit_and_restyle(|edit| edit.set_text(text, LONG));
    let root = fixture.document.root_index().expect("a root element");
    assert_eq!(
        zgui_layout::boxtree::patch::retext(&mut store, &fixture.document, root),
        zgui_layout::boxtree::patch::Retext::Patched(1),
        "the text was replaced rather than rewritten in place, so the box is new and the memo was \
         never asked about it"
    );

    relayout(&mut store, &mut content);
    assert_ne!(
        zgui_layout::tree::print::to_text(&store),
        narrow,
        "the box holding the longer string kept the size the shorter one had"
    );
}

/// Lays the document out again through the gate a frame goes through.
fn relayout(store: &mut LayoutStore, content: &mut Content) {
    let mut tree = LayoutTree::new(store, content, DeviceStyle::default());
    tree.relayout_root(taffy::Size {
        width: VIEWPORT.0,
        height: VIEWPORT.1,
    });
}

/// A grid of `count` panels, each holding a heading and a row of two columns of text.
///
/// The nesting is not decoration. A panel whose children are a heading and a paragraph is measured
/// straight through and asks nothing twice; putting a flex row of columns inside it is what makes
/// the panel's own size a question with an expensive answer, which is then asked once per pass of
/// the track-sizing algorithm above it.
fn panels(count: usize) -> Element {
    let children = (0..count)
        .map(|_| {
            Element::new("panel").children(vec![
                Element::new("h").text("a heading of some length"),
                Element::new("row").children(vec![
                    Element::new("cell")
                        .children(vec![Element::new("p").text(
                            "a paragraph with rather more words in it than the heading",
                        )]),
                    Element::new("cell").children(vec![Element::new("p").text("a shorter one")]),
                ]),
            ])
        })
        .collect();
    Element::new("root").children(children)
}

/// The first text node under `index`.
fn first_text(document: &Document, index: NodeIndex) -> NodeIndex {
    search(document, index).expect("the fixture has a text node")
}

/// The same, answering nothing for a subtree with no text in it.
fn search(document: &Document, index: NodeIndex) -> Option<NodeIndex> {
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
