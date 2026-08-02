//! What one pointer move costs a thousand-row document.
//!
//! Its own target, and the reason is the counter block: it is process-wide, so a case running
//! beside this one would move the numbers this one is asserting on.

mod support;

use std::sync::{Mutex, MutexGuard, PoisonError};

use support::{Element, Fixture, Session};
use zgui_profile::{Counter, counter};
use zgui_vocab::UiState;

/// The counter block is process-wide and the cases in this file all read it, so they take turns.
///
/// Held for the whole case rather than around the measurement alone: work done *outside* a
/// measurement moves the same block, so a case that locked only while measuring would still be
/// reading another case's thousand rows.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A table of `rows` rows, each with one cell, and a rule that styles the hovered row.
fn table(rows: usize) -> Fixture {
    let rows: Vec<Element> = (0..rows)
        .map(|index| {
            Element::new("row")
                .class(if index == 500 { "subject" } else { "other" })
                .children(vec![Element::new("cell")])
        })
        .collect();
    Fixture::new(
        Element::new("root").children(vec![Element::new("table").children(rows)]),
        "root, table { display: block; width: 300px }
         row { display: block; height: 20px }
         cell { display: block; height: 20px }
         row:hover { background-color: rgb(240, 240, 240) }",
    )
}

#[test]
fn hover_on_one_row_restyles_the_row_and_its_ancestors_only() {
    let _measuring = exclusive();
    let mut session = Session::new(table(1000));

    // The first pass styles the document; the budget is about the pointer move that follows.
    session.fixture.restyle();
    let depth = session.fixture.depth_of("cell");
    counter::reset();

    let path = session.hover("cell");
    assert_eq!(
        path.len(),
        depth,
        "the hovered cell and every element above it are on the path"
    );

    let restyled = session.fixture.restyle();
    let after = counter::snapshot();

    // `:hover` is written on the hovered element *and* on every ancestor, because the pointer is
    // over all of them. What that *costs* is a different number: this sheet mentions `:hover` on
    // rows alone, so the writes on the root, the table and the cell match no selector, take the
    // document's cheap path, and never reach the style engine at all. What is styled is the row
    // and what inherits from it. The case below is the same move over a sheet that styles every
    // element on the path, where the cost is the whole path — between them they pin both halves:
    // the bit goes up the chain, and paying for it depends on whether a rule asked.
    assert_eq!(
        restyled, 2,
        "the hovered row and its cell — and none of the 999 rows that were not hovered"
    );
    assert_eq!(after.nodes_relaid_out, 0, "a hover is not a layout");
    assert_eq!(after.text_shaped, 0, "nor a reshape");
    assert_eq!(after.vello_passes, 0, "nor a vector pass");
    assert_eq!(
        after.hit_index_rebuilds, 0,
        "and it does not touch the index it read"
    );
    assert!(
        after.nodes_visited < 64,
        "the walk services the marked path, not the width of the document: {}",
        after.nodes_visited
    );
    assert!(
        after.dirty_walk_steps < 64,
        "and it descends into the marked children rather than testing every child: {}",
        after.dirty_walk_steps
    );

    // And *which* two, because a count of two is also what restyling the wrong two costs: the
    // hovered row's background moved and the identical row beside it did not.
    assert_ne!(
        background_of(&session, session.fixture.find("row")),
        background_of(&session, second_row(&session)),
        "the hovered row is the one whose `:hover` rule now matches"
    );

    // The bit is where the style engine looks for it, on every element of the path.
    for node in &path {
        let index = session
            .fixture
            .document
            .store()
            .index_of(*node)
            .expect("a live element");
        assert!(
            session
                .fixture
                .document
                .store()
                .core(index)
                .ui_state()
                .contains(UiState::HOVER)
        );
    }
}

#[test]
fn moving_between_two_rows_restyles_the_two_and_not_their_shared_ancestors() {
    let _measuring = exclusive();
    // The measurement that separates a hover that diffs its path from one that rewrites it: the
    // table and the root are on both paths and must not be touched at all.
    let rows: Vec<Element> = (0..2)
        .map(|_| Element::new("row").children(vec![Element::new("cell")]))
        .collect();
    let mut session = Session::new(Fixture::new(
        Element::new("root").children(vec![Element::new("table").children(rows)]),
        "root, table { display: block; width: 300px }
         row { display: block; height: 20px }
         cell { display: block; height: 20px }
         row:hover { background-color: rgb(240, 240, 240) }",
    ));
    session.fixture.restyle();

    let first = session.fixture.centre_of("row");
    let second = zgui_geom::Point::new(first.x, zgui_geom::DevicePx(first.y.0 + 20.0));

    session.pointer_at(first, zgui_vocab::PointerAction::Moved);
    session.fixture.restyle();

    counter::reset();
    let path = session.pointer_at(second, zgui_vocab::PointerAction::Moved);
    let restyled = session.fixture.restyle();
    assert_eq!(path.len(), 4, "root, table, row, cell");
    assert_eq!(
        restyled, 4,
        "the row and cell that were left, and the row and cell that were entered — and neither \
         the table nor the root, which the pointer never left"
    );
    assert_eq!(counter::get(Counter::NodesRelaidOut), 0);
}

#[test]
fn a_sheet_that_styles_every_ancestor_on_hover_restyles_one_element_per_ancestor() {
    let _measuring = exclusive();
    // The other half of the budget above. Every element of the hovered path except the root has a
    // `:hover` rule here, so every write on it matters — and the restyled set is exactly those
    // elements, one each. Beside them sits a thousand-row table the pointer never touches, which
    // is what makes "and nothing else" a measurement rather than a hope.
    let rows: Vec<Element> = (0..1000)
        .map(|_| Element::new("row").children(vec![Element::new("cell")]))
        .collect();
    let mut session = Session::new(Fixture::new(
        Element::new("root").children(vec![
            Element::new("outer").children(vec![
                Element::new("middle").children(vec![Element::new("inner")]),
            ]),
            Element::new("table").children(rows),
        ]),
        "root, table { display: block; width: 300px }
         outer, middle, inner { display: block; width: 300px; height: 30px }
         row, cell { display: block; height: 20px }
         outer:hover, middle:hover, inner:hover {
             background-color: rgb(240, 240, 240)
         }",
    ));
    session.fixture.restyle();
    session.fixture.settle();
    counter::reset();

    let path = session.hover("inner");
    let restyled = session.fixture.restyle();
    assert_eq!(
        path,
        vec![
            session.fixture.key("root"),
            session.fixture.key("outer"),
            session.fixture.key("middle"),
            session.fixture.key("inner"),
        ],
        "the pointer is over the deep chain and not over the table beside it"
    );
    assert_eq!(
        restyled, 3,
        "the three elements of the path whose hover a rule asked about — and not the root, whose \
         own write matches nothing, nor any of the two thousand elements of the table"
    );
    assert_eq!(counter::get(Counter::NodesRelaidOut), 0);
    assert_eq!(counter::get(Counter::HitIndexRebuilds), 0);
    assert!(
        counter::get(Counter::DirtyWalkSteps) < 64,
        "and the walk that serviced it did not probe the table's thousand children: {}",
        counter::get(Counter::DirtyWalkSteps)
    );
}

/// The second `row` of the fixture, which is the one the pointer never reaches.
fn second_row(session: &Session) -> zgui_dom::NodeIndex {
    let store = session.fixture.document.store();
    let first = session.fixture.find("row");
    store
        .core(first)
        .next_sibling()
        .expect("the table has more than one row")
}

/// One element's computed background colour, in opaque eight-bit sRGB.
fn background_of(session: &Session, index: zgui_dom::NodeIndex) -> [u8; 3] {
    let style = session
        .fixture
        .document
        .node(index)
        .primary_style()
        .expect("the element was styled");
    let current = zgui_css::values::color::current(&style);
    let resolved = style
        .get_background()
        .background_color
        .resolve_to_absolute(current);
    let colour = zgui_css::values::color::to_color(&resolved);
    let [r, g, b, _] = colour.to_premultiplied_srgb();
    [
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    ]
}
