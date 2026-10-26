//! A line that paints the nested inline boxes on it.

use zgui_layout::fragment::FragmentFlags;
use zgui_layout::{FragmentKind, LayoutStore};

use crate::probe::box_named;
use crate::support::{Element, Fixture, lay_out, measurer};

/// `para` holding `aa ` and a span holding `bbbb`, laid out under `span` declarations.
fn laid(span: &str) -> (LayoutStore, Fixture) {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("para")
                .text("aa ")
                .children(vec![Element::new("span").text("bbbb")]),
        ]),
        &format!(
            "root {{ display: block; width: 400px }}
             para {{ display: block }}
             span {{ display: inline; {span} }}"
        ),
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 600.0);
    (store, fixture)
}

/// The box that holds the paragraph's lines: the anonymous one CSS wraps the run in.
fn context(store: &LayoutStore, fixture: &Fixture) -> zgui_layout::BoxKey {
    let para = box_named(store, fixture, "para");
    store.node(para).children[0]
}

/// The paragraph's first line fragment.
fn first_line<'a>(store: &'a LayoutStore, fixture: &Fixture) -> &'a zgui_layout::Fragment {
    let para = context(store, fixture);
    store
        .fragments_of_box(para)
        .iter()
        .filter_map(|frag| store.fragment(*frag))
        .find(|fragment| matches!(fragment.kind, FragmentKind::Line { line: 0, .. }))
        .expect("a first line")
}

#[test]
fn a_line_holding_a_painted_span_is_flagged_and_inks_the_span() {
    let (plain_store, plain_fixture) = laid("");
    let plain = first_line(&plain_store, &plain_fixture);
    assert!(!plain.flags.contains(FragmentFlags::INLINE_EFFECTS));

    let (store, fixture) = laid("background-color: red; padding: 6px 4px");
    let line = first_line(&store, &fixture);
    assert!(line.flags.contains(FragmentFlags::INLINE_EFFECTS));
    let para = context(&store, &fixture);
    let resolution = store.inline_resolution(para).expect("laid out");
    let content_box = store
        .fragment(store.fragments_of_box(para)[0])
        .expect("live")
        .content_box();
    let part = zgui_layout::inline::slices::device_rect(content_box, &resolution.slices[0]);
    assert!(
        line.local_ink.contains_rect(part),
        "the line's ink {:?} holds the span {part:?}",
        line.local_ink
    );
    assert!(
        part.origin.y.0 < plain.local_ink.origin.y.0 + 6.0,
        "the padding reaches past the plain line's ink"
    );
    assert_ne!(
        store.fragment_extra(line).content_hash,
        0,
        "the part's place is in the fingerprint"
    );
}

#[test]
fn padding_alone_paints_nothing_and_leaves_the_line_unflagged() {
    let (store, fixture) = laid("padding: 0 4px; color: blue");
    let line = first_line(&store, &fixture);
    assert!(!line.flags.contains(FragmentFlags::INLINE_EFFECTS));
    assert_eq!(store.fragment_extra(line).content_hash, 0);
}

#[test]
fn a_decoration_or_a_text_shadow_of_its_own_flags_the_line() {
    for declarations in [
        "text-decoration-line: underline",
        "text-shadow: 1px 1px red",
        "border: 1px solid black",
        "box-shadow: 0 0 2px black",
        "outline: 2px solid black",
    ] {
        let (store, fixture) = laid(declarations);
        assert!(
            first_line(&store, &fixture)
                .flags
                .contains(FragmentFlags::INLINE_EFFECTS),
            "{declarations}"
        );
    }
}

#[test]
fn a_nested_inline_box_paints_nothing_of_its_own() {
    // The lines paint its parts. A box of its own would put its shadow and its outline at the
    // place layout put the box, which is the start of the context's content.
    let (store, fixture) = laid("box-shadow: 0 0 8px black; outline: 2px solid black");
    let span = box_named(&store, &fixture, "span");
    let own = store
        .fragment(store.fragments_of_box(span)[0])
        .expect("the span has a fragment");
    assert_eq!(own.kind, FragmentKind::Box);
    assert!(own.flags.contains(FragmentFlags::PAINTS_NOTHING));
    assert!(own.local_ink.is_empty(), "{:?}", own.local_ink);
    assert!(own.ink.is_empty(), "{:?}", own.ink);
}
