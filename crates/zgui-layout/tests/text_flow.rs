//! What happens to the document's own characters on the way to a shaper.

mod support;

use support::{Element, Fixture, lay_out, measurer};
use zgui_layout::tree::store::LayoutStore;
use zgui_text::{ParagraphKey, SourcePos};

/// One character's advance at the initial font size.
const ADVANCE: f32 = 8.0;

/// The first box that establishes an inline formatting context.
fn inline_root(store: &LayoutStore) -> zgui_layout::BoxKey {
    let mut stack = vec![store.root().expect("a root")];
    while let Some(key) = stack.pop() {
        if store.inline_resolution(key).is_some() {
            return key;
        }
        stack.extend(store.node(key).children.iter().copied());
    }
    panic!("no inline formatting context was laid out");
}

/// Lays out one paragraph of `text` under `css` and returns the string the shaper was handed.
fn generated(text: &'static str, css: &str) -> (String, ParagraphKey, support::Content) {
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").text(text)]),
        css,
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 600.0);
    let key = store
        .inline_resolution(inline_root(&store))
        .expect("laid out")
        .key;
    let shaped = content.cache().get(key).expect("the paragraph was shaped");
    (shaped.text().to_owned(), key, content)
}

/// The default stylesheet these fixtures share.
const PLAIN: &str = "root { display: block; width: 400px }
     para { display: block }";

#[test]
fn a_run_of_white_space_becomes_one_space_and_the_edges_are_dropped() {
    for (source, expected) in [
        ("  leading", "leading"),
        ("trailing  ", "trailing"),
        ("a  \n\t b", "a b"),
        ("a\n\nb", "a b"),
        ("   ", ""),
        ("one two", "one two"),
    ] {
        let (text, _, _) = generated(source, PLAIN);
        assert_eq!(text, expected, "collapsing {source:?}");
    }
}

#[test]
fn preserved_white_space_survives_exactly_as_written() {
    let (text, _, _) = generated(
        "  a  b  ",
        "root { display: block; width: 400px }
         para { display: block; white-space: pre }",
    );
    assert_eq!(text, "  a  b  ");
}

#[test]
fn a_forced_break_ends_a_line_and_the_next_one_starts_clean() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").text("a\n  b")]),
        "root { display: block; width: 400px }
         para { display: block; white-space: pre-line }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 600.0);
    let resolution = store
        .inline_resolution(inline_root(&store))
        .expect("laid out");
    assert_eq!(resolution.lines.len(), 2, "the newline broke the line");
    // The white space after the break was dropped rather than collapsed to a space, so the second
    // line is one character wide.
    assert_eq!(resolution.lines[1].width, ADVANCE);
}

#[test]
fn every_generated_offset_maps_back_to_the_character_it_came_from() {
    // The map is what every caret, selection and hit test is remapped through, and collapsing is
    // exactly what makes it necessary: the fourth character of the generated string is not the
    // fourth character of the source.
    let source = "  the   quick  ";
    let (text, key, content) = generated(source, PLAIN);
    assert_eq!(text, "the quick");
    let shaped = content.cache().get(key).expect("shaped");
    let map = shaped.map();

    // Every generated offset has a source position, and mapping it forward again lands on the same
    // generated offset — with one exception, which is the point of the test: the collapsed space
    // maps back to the first byte of a run the source spelled with three.
    for (offset, character) in text.char_indices() {
        let position = map
            .to_source(offset)
            .unwrap_or_else(|| panic!("generated offset {offset} has no source"));
        assert!(
            source[position.offset..].starts_with(character)
                || (character == ' ' && source[position.offset..].starts_with(' ')),
            "generated {offset} maps to source {} which is not {character:?}",
            position.offset
        );
    }
    // The three characters of source white space all map into one generated space.
    assert_eq!(map.to_source(3), Some(SourcePos { run: 0, offset: 5 }));
    assert_eq!(map.to_generated(SourcePos { run: 0, offset: 2 }), Some(0));
    // And the leading white space the document holds survived nowhere at all.
    assert_eq!(map.to_generated(SourcePos { run: 0, offset: 0 }), None);
}

#[test]
fn white_space_either_side_of_something_opaque_survives_as_one_space_each_side() {
    let fixture = Fixture::with_natural_size(
        Element::new("root").children(vec![Element::new("para").children(vec![
            Element::new("lead").text("a  "),
            Element::new("picture").image(20.0, 10.0),
            Element::new("tail").text("  b"),
        ])]),
        "root { display: block; width: 400px }
         para { display: block }",
        (20.0, 10.0),
    );
    let mut store = fixture.box_tree();
    let mut content = support::measurer_with_images(20.0, 10.0);
    lay_out(&mut store, &mut content, 400.0, 600.0);
    let key = store
        .inline_resolution(inline_root(&store))
        .expect("laid out")
        .key;
    let shaped = content.cache().get(key).expect("shaped");
    assert_eq!(shaped.text(), "a  b", "one space each side of the image");
}

#[test]
fn a_word_that_does_not_fit_overflows_unless_it_is_allowed_to_break() {
    let long = "abcdefghijklmnopqrstuvwxyz";
    let (_, _, _) = generated(long, PLAIN);

    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").text(long)]),
        "root { display: block; width: 60px }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 60.0, 600.0);
    let resolution = store
        .inline_resolution(inline_root(&store))
        .expect("laid out");
    assert_eq!(resolution.lines.len(), 1, "the word cannot be broken");
    assert!(
        resolution.lines[0].width > 60.0,
        "so it overflows the line rather than being cut"
    );
}
