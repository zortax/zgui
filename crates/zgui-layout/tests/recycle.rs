//! A window of rows scrolled by one: one row arrives, one leaves, and the rest do not move.
//!
//! This is the shape a virtualised list mutates in, and it is the shape a whole-container rebuild is
//! most expensive in: the container is the port, so "rebuild the container that gained a child"
//! rebuilds every row on the screen to service the one that arrived. What is asserted here is that
//! the rows nobody touched keep the boxes they had — their names are what fragment reuse, geometry
//! diffing and damage scissoring are keyed on — and that the tree left behind is the tree a build
//! from the document root produces.
//!
//! Every case holds a counter recording, because the counters are one process-wide block and a build
//! running on another thread of this binary would otherwise land in whatever measurement is open.

mod support;

use support::{Element, Fixture};
use zgui_dom::side::BoxKey;
use zgui_dom::{NodeIndex, NodeKind};
use zgui_layout::boxtree::Owed;
use zgui_layout::boxtree::patch;
use zgui_layout::tree::print::to_text;
use zgui_layout::tree::store::LayoutStore;
use zgui_profile::Counter;
use zgui_testkit_scene::counters::Recording;

/// How many rows the port holds.
const ROWS: usize = 8;

/// A column of `ROWS` rows, each holding one cell with text in it.
///
/// Named `row0`, `row1` and so on, because a splice has to be asked about *particular* rows: the
/// claim is that the rows nobody touched kept their boxes, which is a claim about identity.
fn port() -> Fixture {
    let rows: Vec<Element> = (0..ROWS)
        .map(|index| {
            Element::new(ROW_NAMES[index])
                .classes(&["row"])
                .children(vec![Element::new("cell").text("row")])
        })
        .collect();
    Fixture::new(
        Element::new("scroller").children(vec![Element::new("pane").children(rows)]),
        "scroller { display: block; width: 400px }
         pane { display: flex; flex-direction: column; padding-top: 40px }
         .row { display: block; height: 24px }
         cell { display: block }",
    )
}

/// The names the fixture's rows carry, in order.
const ROW_NAMES: [&str; 16] = [
    "row0", "row1", "row2", "row3", "row4", "row5", "row6", "row7", "row8", "row9", "row10",
    "row11", "row12", "row13", "row14", "row15",
];

/// The element named `name`.
///
/// # Panics
///
/// Panics when nothing in the document carries the name, because the caller is about to assert
/// something about it and `None` would make that assertion pass by never running.
fn element(fixture: &Fixture, name: &str) -> NodeIndex {
    let store = fixture.document.store();
    let mut stack = vec![fixture.root];
    while let Some(index) = stack.pop() {
        if store.core(index).kind() == NodeKind::Element
            && store.core(index).local_name().as_str() == name
        {
            return index;
        }
        let mut next = store.core(index).first_child();
        while let Some(child) = next {
            stack.push(child);
            next = store.core(child).next_sibling();
        }
    }
    panic!("the fixture has no `{name}` element");
}

/// The boxes one element generated, by name.
fn boxes(store: &LayoutStore, fixture: &Fixture, name: &str) -> Vec<BoxKey> {
    let key = fixture.document.store().key_of(element(fixture, name));
    store.boxes_of(key).to_vec()
}

/// A tree rendering with the box numbers taken out of it.
fn shape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("paint-order=[") {
        out.push_str(&rest[..at]);
        out.push_str("paint-order=[..]");
        let after = &rest[at..];
        let end = after.find(']').map_or(after.len(), |index| index + 1);
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// Scrolls the window by one row: `leaving` goes from the top and `arriving` appears at the bottom.
///
/// The row that leaves is unmounted the way a view unmounts, which is one node at a time from the
/// inside out. That is not decoration: it leaves every node of the departed row with no parent at
/// all, so nothing about the row can be established by asking what is above it — and a proof of
/// confinement written in terms of ancestry answers "this box was written somewhere else" for every
/// box in it.
fn shift(fixture: &mut Fixture, leaving: &str, arriving: &'static str) -> NodeIndex {
    let pane = element(fixture, "pane");
    let going = element(fixture, leaving);
    let inner = fixture
        .document
        .store()
        .core(going)
        .first_child()
        .expect("every row of the fixture holds a cell");
    fixture.edit_and_restyle(|edit| {
        edit.remove(inner);
        edit.remove(going);
        let row = edit.create_element(zgui_interned::ElementName::new(arriving));
        edit.set_classes(row, &[zgui_interned::ClassName::new("row")]);
        let cell = edit.create_element(zgui_interned::ElementName::new("cell"));
        let text = edit.create_text("row");
        edit.insert_before(cell, text, None);
        edit.insert_before(row, cell, None);
        edit.insert_before(pane, row, None);
    });
    pane
}

/// What a container that gained and lost a child owes.
fn owes_children(pane: NodeIndex) -> Owed {
    Owed {
        rebuilt: Vec::new(),
        children: vec![pane],
    }
}

#[test]
fn one_row_of_travel_rebuilds_one_row() {
    let mut recording = Recording::begin();
    let mut fixture = port();
    let mut store = fixture.box_tree();

    // Every row that is going to stay, by the boxes it has now.
    let kept: Vec<Vec<BoxKey>> = (1..ROWS)
        .map(|index| boxes(&store, &fixture, ROW_NAMES[index]))
        .collect();

    let pane = shift(&mut fixture, "row0", "row8");
    let spliced = recording.measure(|| {
        let done = patch::rebuild(&mut store, &fixture.document, &owes_children(pane))
            .expect("a row arriving in a column of rows is confined");
        assert_eq!(done.subtrees, 1, "one container was serviced");
    });

    // The row that arrived is a block, a cell and a run of text: three boxes and no more. The bound
    // is what the whole scenario turns on — a splice that rebuilt the port would count the rows that
    // did not change with it.
    assert_eq!(
        spliced.get(Counter::BoxesRebuilt),
        3,
        "the splice built more than the row that arrived"
    );
    for (index, before) in kept.iter().enumerate() {
        assert_eq!(
            &boxes(&store, &fixture, ROW_NAMES[index + 1]),
            before,
            "a row nobody touched was given new boxes, so every fragment in it compares as changed"
        );
    }
    assert_eq!(
        shape(&to_text(&store)),
        shape(&to_text(&fixture.box_tree())),
        "the spliced tree is not the tree a build from the root produces"
    );
}

#[test]
fn eight_rows_of_travel_leave_the_tree_a_build_would_leave() {
    let _recording = Recording::begin();
    let mut fixture = port();
    let mut store = fixture.box_tree();

    for step in 0..ROWS {
        let pane = shift(&mut fixture, ROW_NAMES[step], ROW_NAMES[ROWS + step]);
        patch::rebuild(&mut store, &fixture.document, &owes_children(pane))
            .expect("every row of travel is confined");
        assert_eq!(
            shape(&to_text(&store)),
            shape(&to_text(&fixture.box_tree())),
            "the tree drifted from the one a build produces after {} shifts",
            step + 1
        );
    }
}

#[test]
fn a_pane_holding_a_run_of_text_beside_its_rows_is_rebuilt_whole() {
    let _recording = Recording::begin();
    // The pane's own text makes it a block container with an inline run in it, and a run is broken
    // into anonymous boxes that belong to the pane. Where a row lands among those is the pane's
    // decision, so the narrow path has to decline and the whole subtree be made again.
    let mut fixture = Fixture::new(
        Element::new("scroller").children(vec![Element::new("pane").text("caption").children(
            vec![
                Element::new("row0")
                    .classes(&["row"])
                    .children(vec![Element::new("cell").text("row")]),
                Element::new("row1")
                    .classes(&["row"])
                    .children(vec![Element::new("cell").text("row")]),
            ],
        )]),
        "scroller { display: block; width: 400px }
         pane { display: block }
         .row { display: block; height: 24px }
         cell { display: block }",
    );
    let mut store = fixture.box_tree();
    let before = shape(&to_text(&store));
    let pane = shift(&mut fixture, "row0", "row8");
    assert!(
        patch::rebuild(&mut store, &fixture.document, &owes_children(pane)).is_none(),
        "a row was spliced into a container whose anonymous boxes it re-breaks"
    );
    assert_eq!(
        shape(&to_text(&store)),
        before,
        "a refused splice left the tree changed, so the build that follows it starts from a tree \
         that is neither the old one nor the new one"
    );
}

#[test]
fn a_row_that_arrives_inline_level_is_refused_rather_than_left_unwrapped() {
    let _recording = Recording::begin();
    // An inline-level child of a block container is swept into an anonymous inline box with whatever
    // inline siblings it has. The rows already in the pane carry no mark, so a splice that put this
    // one in beside them would leave the wrapping the old tree had.
    let mut fixture = Fixture::new(
        Element::new("scroller").children(vec![Element::new("pane").children(vec![
                Element::new("row0")
                    .classes(&["row"])
                    .children(vec![Element::new("cell").text("row")]),
                Element::new("row1")
                    .classes(&["row"])
                    .children(vec![Element::new("cell").text("row")]),
            ])]),
        "scroller { display: block; width: 400px }
         pane { display: block }
         .row { display: block; height: 24px }
         row8.row { display: inline }
         cell { display: block }",
    );
    let mut store = fixture.box_tree();
    let before = shape(&to_text(&store));
    let pane = shift(&mut fixture, "row0", "row8");
    assert!(
        patch::rebuild(&mut store, &fixture.document, &owes_children(pane)).is_none(),
        "an inline-level row was spliced in beside blocks with no anonymous box around it"
    );
    assert_eq!(
        shape(&to_text(&store)),
        before,
        "a refused splice left the tree changed"
    );
}

#[test]
fn a_row_whose_children_take_its_place_is_refused_rather_than_dropped() {
    let _recording = Recording::begin();
    // `display: contents` puts the arriving row's *cell* into the pane's child list and the row
    // itself nowhere. One child is then not one box, which is the correspondence the narrow path
    // matches its steps by, so it declines.
    let mut fixture = Fixture::new(
        Element::new("scroller").children(vec![Element::new("pane").children(vec![
                Element::new("row0")
                    .classes(&["row"])
                    .children(vec![Element::new("cell").text("row")]),
                Element::new("row1")
                    .classes(&["row"])
                    .children(vec![Element::new("cell").text("row")]),
            ])]),
        "scroller { display: block; width: 400px }
         pane { display: flex; flex-direction: column }
         .row { display: block; height: 24px }
         row8.row { display: contents }
         cell { display: block }",
    );
    let mut store = fixture.box_tree();
    let before = shape(&to_text(&store));
    let pane = shift(&mut fixture, "row0", "row8");
    assert!(
        patch::rebuild(&mut store, &fixture.document, &owes_children(pane)).is_none(),
        "a row whose children take its place was spliced in as one box of its own"
    );
    assert_eq!(
        shape(&to_text(&store)),
        before,
        "a refused splice left the tree changed"
    );
}

#[test]
fn a_row_holding_a_box_the_scroller_lays_out_refuses_the_splice() {
    let _recording = Recording::begin();
    // The cell inside each row is positioned against the `scroller`, so its box is a layout child of
    // the scroller's and a paint child of its row's. Taking a row's subtree out reaches that box
    // through the paint order and destroys it, and the scroller would go on naming it.
    let mut fixture = Fixture::new(
        Element::new("scroller").children(vec![Element::new("pane").children(vec![
                Element::new("row0")
                    .classes(&["row"])
                    .children(vec![Element::new("cell").text("row")]),
                Element::new("row1")
                    .classes(&["row"])
                    .children(vec![Element::new("cell").text("row")]),
            ])]),
        "scroller { display: block; width: 400px; position: relative }
         pane { display: flex; flex-direction: column }
         .row { display: block; height: 24px }
         cell { display: block; position: absolute; top: 0; left: 0 }",
    );
    let mut store = fixture.box_tree();
    let before = shape(&to_text(&store));
    let pane = shift(&mut fixture, "row0", "row8");
    assert!(
        patch::rebuild(&mut store, &fixture.document, &owes_children(pane)).is_none(),
        "a subtree holding a box its own container does not lay out was taken out anyway"
    );
    assert_eq!(
        shape(&to_text(&store)),
        before,
        "a refused splice left the tree changed"
    );
}

// -- the slots a rebuild gives back ---------------------------------------------------------------

/// A document rebuilt over and over settles at a bounded number of slots.
///
/// `boxtree::build` removes every box before it builds, and a removed box stays readable — and its
/// slot withheld — until the frame that removed it is recycled. That is what makes a box key safe
/// to hold across the stages of one frame, and it means the two documents overlap: a rebuild peaks
/// at the old tree plus the new one, so the arena settles at twice the document and stays there.
///
/// Staying there is the claim. Without the recycling, every rebuild leaves a whole document's worth
/// of `BoxNode`s undropped — each holding a `ComputedStyle`, its text and two child lists — and the
/// arena climbs by the size of the document every time the patch path refuses, for the life of the
/// process.
///
/// The number to watch is the arena's *capacity* rather than how many boxes are live. The live
/// count reads flat straight through this failure: the document really does hold the same boxes
/// afterwards, and it is the slots behind them that never come back.
#[test]
fn rebuilding_a_document_over_and_over_does_not_grow_the_box_arena() {
    let _recording = Recording::begin();
    let fixture = port();
    let mut store = LayoutStore::new(fixture.document.store().document());

    zgui_layout::boxtree::build(&mut store, &fixture.document);
    store.recycle();
    let live = store.len();
    assert!(live > 0, "the fixture built no boxes");

    // The first rebuild is the one that peaks: the outgoing tree is still holding its slots while
    // the incoming one is allocated. Every rebuild after it reuses what that one gave back.
    zgui_layout::boxtree::build(&mut store, &fixture.document);
    store.recycle();
    let settled = store.box_capacity();
    assert!(
        settled <= live * 2,
        "one rebuild reached {settled} slots for a document of {live} boxes, which is more than \
         the two trees that briefly coexist"
    );

    for round in 2..20 {
        zgui_layout::boxtree::build(&mut store, &fixture.document);
        store.recycle();
        assert_eq!(
            store.len(),
            live,
            "round {round} built a different number of boxes"
        );
        assert_eq!(
            store.box_capacity(),
            settled,
            "round {round} took slots the arena should have had free, so the boxes removed by the \
             rounds before it were never given back"
        );
    }
}

/// A removed box keeps its slot until the frame is recycled.
///
/// The other half of the same contract, and the reason the recycling is at the end of the frame
/// rather than inside the layout pass: every stage after layout — scroll dispatch, geometry
/// observations, re-hit, the caret, paint, the accessibility walk — may still resolve a box key it
/// captured earlier in the frame, and a slot handed back early is a key that has silently come to
/// mean something else.
#[test]
fn a_removed_box_keeps_its_slot_until_the_frame_is_recycled() {
    let _recording = Recording::begin();
    let fixture = port();
    let mut store = LayoutStore::new(fixture.document.store().document());
    zgui_layout::boxtree::build(&mut store, &fixture.document);
    let key = store.root().expect("a root");
    assert!(store.contains(key));

    // Removing it does not take it away: the rest of the frame can still read it.
    store.remove(key);
    assert!(
        store.get(key).is_some(),
        "a box removed mid-frame stopped resolving before the frame ended"
    );

    store.recycle();
    assert!(
        !store.contains(key),
        "the recycling left the removed box resolvable, so its slot was never given back"
    );
}
