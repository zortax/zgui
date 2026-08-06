//! Every list in this library that scrolls says which way.
//!
//! CSS computes an `overflow` of `visible` to `auto` when the other axis is not `visible`, so a box
//! that says `overflow-y: auto` and leaves `overflow-x` alone has asked for **both** bars. Whether
//! one then appears is up to the content, and a list's content overflows sideways more easily than
//! it looks: a group separator reaches its own padding's width past both edges on purpose, which is
//! what draws a rule right across a menu rather than a line inside it, and a vertical bar takes its
//! gutter out of the width the rows were sized against.
//!
//! So a sideways bar under a list of currencies is not a strange thing that happened once. It is
//! what every scrolling list here does unless it says otherwise — and most of them do say, which is
//! exactly why the ones that had not were hard to spot.
//!
//! The check is on the style sheets rather than on a window, because that is where the answer is. A
//! fixture that opened each list and looked for a bar would be asking whether *that* content, at
//! *that* size, on *that* display scale, happened to overflow — three conditions the defect is
//! sensitive to and none of which is the thing being claimed.

use zgui_ui::{
    ComboboxStyle, CommandStyle, DataTableStyle, MenuStyle, NativeSelectStyle, SelectStyle,
    VirtualListStyle,
};

/// Every style sheet in the library that makes something scroll, and what it is called.
///
/// Hand-written, and the assertion below is what stops it falling behind: a sheet named here that
/// has stopped scrolling fails, so a list that moves out of this file has to be taken out of it
/// deliberately rather than by being forgotten.
fn scrolling_sheets() -> Vec<(&'static str, &'static str)> {
    vec![
        ("select", SelectStyle::CSS),
        ("native select", NativeSelectStyle::CSS),
        ("combobox", ComboboxStyle::CSS),
        ("menu", MenuStyle::CSS),
        ("command", CommandStyle::CSS),
        ("data table", DataTableStyle::CSS),
        ("virtualiser", VirtualListStyle::CSS),
    ]
}

/// The rule block each `overflow-y: auto` sits in, as text.
///
/// Blocks are found by brace rather than parsed: a declaration belongs to the rule whose `{` most
/// recently opened before it, which is all this needs to know and all a style sheet in this crate
/// is ever shaped like.
fn blocks_that_scroll_down(css: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = css;
    while let Some(at) = rest.find("overflow-y: auto") {
        let open = rest[..at].rfind('{').map_or(0, |brace| brace + 1);
        let close = rest[at..].find('}').map_or(rest.len(), |brace| at + brace);
        found.push(&rest[open..close]);
        rest = &rest[close..];
    }
    found
}

#[test]
fn every_list_that_scrolls_down_says_whether_it_scrolls_sideways() {
    for (name, css) in scrolling_sheets() {
        let blocks = blocks_that_scroll_down(css);
        assert!(
            !blocks.is_empty(),
            "the {name} sheet is listed here as something that scrolls and no longer does; take it \
             out of the list rather than leaving it to pass vacuously"
        );
        for block in blocks {
            assert!(
                block.contains("overflow-x:"),
                "the {name} sheet scrolls down without saying what it does sideways, so it has \
                 asked for a horizontal scrollbar as well:\n{block}"
            );
        }
    }
}

#[test]
fn a_block_is_found_by_the_brace_it_is_inside() {
    // The one thing the reading above could get wrong, stated on text this file controls.
    let css = ".a { color: red }
               .b { overflow-y: auto; overflow-x: hidden }
               .c { overflow-y: auto }";
    let blocks = blocks_that_scroll_down(css);
    assert_eq!(blocks.len(), 2);
    assert!(blocks[0].contains("overflow-x: hidden"));
    assert!(!blocks[1].contains("overflow-x"));
}
