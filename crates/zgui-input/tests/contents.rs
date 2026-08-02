//! The chain is the element ancestry, not the box ancestry.
//!
//! `tr:hover td` is the case, and it is not a curiosity: a table row is routinely
//! `display: contents` so that its cells become children of a grid, and a component library that
//! could not style a row on hover would be missing the commonest table there is. The row generates
//! no box at all, so nothing on the box path from the cell passes through it — and the rule still
//! has to match, because the pointer is over the row in every sense that matters to a person.

mod support;

use support::{Element, Fixture, Session};
use zgui_vocab::UiState;

/// A grid whose rows are `display: contents`, so the cells are the grid's own children.
fn table() -> Fixture {
    Fixture::new(
        Element::new("root").children(vec![Element::new("table").children(vec![
            Element::new("row").children(vec![Element::new("cell"), Element::new("cell")]),
        ])]),
        "root, table { display: block; width: 300px }
         row { display: contents }
         cell { display: block; height: 20px }
         row:hover cell { background-color: rgb(240, 240, 240) }",
    )
}

#[test]
fn a_row_that_generates_no_box_is_still_on_the_chain() {
    let fixture = table();
    let row = fixture.key("row");
    assert!(
        fixture.layout.boxes_of(row).is_empty(),
        "the fixture is only a test of this if the row really generates no box"
    );

    let mut session = Session::new(fixture);
    let path = session.hover("cell");

    let row = session.fixture.key("row");
    assert!(
        path.contains(&row),
        "the chain walks the document, so the row is on it however few boxes it generated"
    );
    assert_eq!(
        path,
        vec![
            session.fixture.key("root"),
            session.fixture.key("table"),
            row,
            session.fixture.key("cell"),
        ]
    );

    let index = session
        .fixture
        .document
        .store()
        .index_of(row)
        .expect("a live element");
    assert!(
        session
            .fixture
            .document
            .store()
            .core(index)
            .ui_state()
            .contains(UiState::HOVER),
        "and it carries the bit, which is what `tr:hover td` matches on"
    );
}

#[test]
fn tr_hover_td_matches_over_a_contents_row() {
    // The whole point of the chain being the element ancestry, asserted where it is visible: the
    // cascade, not the state word.
    let mut session = Session::new(table());
    session.fixture.restyle();
    session.fixture.settle();

    let before = colour_of(&session, "cell");
    session.hover("cell");
    let styled = session.fixture.restyle();
    let after = colour_of(&session, "cell");

    assert!(styled > 0, "hovering the cell restyled something");
    assert_ne!(
        before, after,
        "the descendant rule under `row:hover` now matches, so the cell's background moved"
    );
    assert_eq!(after, [240, 240, 240]);
}

/// The shape a real data table has: a boxless section holding boxless rows, whose cells are the
/// grid's own items, with the rules written the way the component library writes them.
///
/// Two levels of `display: contents` rather than one, because that is what a table with a header
/// and a body actually is — and a chain that flattened one level and lost the other would pass the
/// single-level case above while the component that ships fails.
fn sections() -> Fixture {
    Fixture::new(
        Element::new("root").children(vec![Element::new("grid").class("zui-table").children(
            vec![
                Element::new("body")
                    .class("zui-table__section")
                    .children(vec![
                        Element::new("row").class("zui-table__row").children(vec![
                            Element::new("cell").class("zui-table__cell"),
                            Element::new("cell").class("zui-table__cell"),
                        ]),
                    ]),
            ],
        )]),
        "root { display: block; width: 300px }
         .zui-table { display: grid; grid-template-columns: 1fr 1fr }
         .zui-table__section { display: contents }
         .zui-table__row { display: contents }
         .zui-table__cell { display: block; height: 20px }
         .zui-table__row:hover .zui-table__cell { background-color: rgb(240, 240, 240) }",
    )
}

#[test]
fn a_row_inside_a_boxless_section_still_matches_on_hover() {
    let fixture = sections();
    for boxless in ["body", "row"] {
        assert!(
            fixture.layout.boxes_of(fixture.key(boxless)).is_empty(),
            "`{boxless}` generates a box, so this fixture tests nothing",
        );
    }

    let mut session = Session::new(sections());
    session.fixture.restyle();
    session.fixture.settle();
    let before = colour_of(&session, "cell");

    let path = session.hover("cell");
    session.fixture.restyle();

    assert!(
        path.contains(&session.fixture.key("body")),
        "the section is boxless too, and it is still on the chain",
    );
    assert_eq!(
        colour_of(&session, "cell"),
        [240, 240, 240],
        "the row's hover rule did not reach its cells through two boxless levels",
    );
    assert_ne!(before, colour_of(&session, "cell"));
}

/// One element's computed background colour, in opaque eight-bit sRGB.
fn colour_of(session: &Session, name: &str) -> [u8; 3] {
    let index = session.fixture.find(name);
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
