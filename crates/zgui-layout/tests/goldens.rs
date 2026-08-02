//! Box-tree and resolved-layout goldens.
//!
//! These are the primary evidence that the box tree and the layout over it are right: a change to
//! either shows up as a diff in a text file that a person reads, rather than as a number in an
//! assertion nobody wrote.

mod support;

use std::path::PathBuf;

use support::{Element, Fixture, lay_out, measurer};
use zgui_testkit_scene::dump::{TreeDump, golden};
use zgui_testkit_scene::text::Writer;

/// A box tree and its resolved layout, as the shared tree-dump seam wants it.
///
/// The text is produced by the layout crate itself and written through here a line at a time,
/// because the tree-dump seam belongs to the test harness and an engine that depended on its own
/// harness could not be brought up without it.
struct BoxTree(String);

impl TreeDump for BoxTree {
    fn dump(&self, writer: &mut Writer) {
        for line in self.0.lines() {
            writer.line(line);
        }
    }
}

/// Where the goldens live.
fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(format!("{name}.txt"))
}

/// Builds, lays out and compares one fixture against its golden.
fn check(name: &str, tree: Element, css: &str, width: f32, height: f32) {
    let fixture = Fixture::new(tree, css);
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, width, height);
    let rendering = zgui_layout::tree::print::to_text(&store);
    // The same tree has to render the same bytes every time, or a golden is noise rather than
    // evidence.
    assert_eq!(rendering, zgui_layout::tree::print::to_text(&store));
    golden::assert_tree(&golden_path(name), &BoxTree(rendering));
}

#[test]
fn block_flow() {
    check(
        "block-flow",
        Element::new("root").children(vec![
            Element::new("head").text("title"),
            Element::new("body").children(vec![Element::new("para").text("words")]),
        ]),
        "root { display: block; width: 300px }
         head { display: block; height: 40px }
         body { display: block; padding: 10px; border: 2px solid black }
         para { display: block }",
        300.0,
        400.0,
    );
}

#[test]
fn flexbox() {
    check(
        "flexbox",
        Element::new("root").children(vec![
            Element::new("a").text("one"),
            Element::new("b").text("two"),
            Element::new("c").text("three"),
        ]),
        "root { display: flex; width: 300px; height: 100px; gap: 8px; align-items: center }
         a { flex-grow: 1 }
         b { flex-grow: 2; order: -1 }
         c { width: 40px }",
        300.0,
        400.0,
    );
}

#[test]
fn grid() {
    check(
        "grid",
        Element::new("root").children(vec![
            Element::new("a").text("one"),
            Element::new("b").text("two"),
            Element::new("c").text("three"),
        ]),
        "root { display: grid; width: 300px;
                grid-template-columns: [start] 100px 1fr [end];
                grid-template-rows: 50px 50px; gap: 4px }
         a { grid-column-start: 1 }
         b { grid-column-start: 2 }
         c { grid-column-start: span 2 }",
        300.0,
        400.0,
    );
}

#[test]
fn a_generated_content_box_comes_first_in_paint_order() {
    // `::before` is the only producer of a pseudo box, and the box has to be the first thing
    // painted inside its originating element — a recording of the document cannot see any of this,
    // because there is no node to record.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("item").classes(&["item"]).text("body")]),
        "root { display: block; width: 200px }
         .item { display: block }
         .item::before { content: \"\\2022\"; display: block }",
    );
    let store = fixture.box_tree();
    let root = store.root().expect("a root");
    let item = store.node(root).children[0];
    let first = store.node(item).paint_children[0];
    assert_eq!(
        store.node(first).pseudo,
        Some(zgui_layout::PseudoKind::Before),
        "the generated box is not first in paint order"
    );
    // It names the element it was generated from, because it has none of its own.
    assert_eq!(
        store.node(first).source,
        store.node(item).source,
        "a generated box names its originating element"
    );
    // And the content it places is a text run below it.
    let run = store.node(first).children[0];
    assert_eq!(store.node(run).text.as_deref(), Some("\u{2022}"));
}

#[test]
fn a_content_toggle_makes_the_generated_box_appear_and_disappear() {
    // The failure this guards against is a `::before` whose existence is decided once and then
    // cached: a class toggle that changes `content` has to add a box that was not there.
    //
    // The off state is `content: none` rather than the empty string, because an empty string is a
    // content item and does generate a box — an empty one, which is what a browser produces too.
    let css = "root { display: block; width: 200px }
               .item { display: block }
               .item::before { content: none }
               .item.on::before { content: \"\\2713\" }";
    let mut fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("item").classes(&["item"]).text("body")]),
        css,
    );

    let store = fixture.box_tree();
    let item = store.node(store.root().expect("a root")).children[0];
    assert!(
        generated(&store, item).is_none(),
        "`content: none` generates no box"
    );

    let target = fixture
        .document
        .store()
        .core(fixture.root)
        .first_child()
        .expect("the item");
    fixture.edit_and_restyle(|edit| {
        edit.add_class(target, zgui_interned::ClassName::new("on"));
    });

    let store = fixture.box_tree();
    let item = store.node(store.root().expect("a root")).children[0];
    let generated = generated(&store, item).expect("the toggle made a box appear");
    assert_eq!(
        store.node(generated).pseudo,
        Some(zgui_layout::PseudoKind::Before)
    );
    let run = store.node(generated).children[0];
    assert_eq!(store.node(run).text.as_deref(), Some("\u{2713}"));
}

/// The first generated-content box below `key` in paint order, if there is one.
fn generated(
    store: &zgui_layout::LayoutStore,
    key: zgui_layout::BoxKey,
) -> Option<zgui_layout::BoxKey> {
    if store.node(key).pseudo.is_some() {
        return Some(key);
    }
    store
        .node(key)
        .paint_children
        .iter()
        .find_map(|&child| generated(store, child))
}

#[test]
fn a_list_item_produces_a_marker_box() {
    // The mark is a box of its own with no element and no pseudo-element: the pseudo-element that
    // would carry author styling for it is resolved lazily and no traversal computes one, while the
    // properties that decide the default mark are inherited properties of the item itself.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("item").text("first")]),
        "root { display: block; width: 200px }
         item { display: list-item }",
    );
    let store = fixture.box_tree();
    let root = store.root().expect("a root");
    let item = store.node(root).children[0];
    let mut marks = Vec::new();
    let mut stack = vec![item];
    while let Some(key) = stack.pop() {
        if store.node(key).kind == zgui_layout::BoxKind::Marker {
            marks.push(key);
        }
        stack.extend(store.node(key).paint_children.iter().copied());
    }
    assert_eq!(marks.len(), 1, "a list item marks itself exactly once");
    let mark = marks[0];
    assert_eq!(
        store.node(mark).pseudo,
        None,
        "the mark is not a pseudo box"
    );
    assert_eq!(store.node(mark).text.as_deref(), Some("\u{2022}"));
    assert_eq!(store.node(mark).source, store.node(item).source);
}

/// The lines an inline formatting context resolved to, and the fragments they became.
struct InlineLines(String);

impl TreeDump for InlineLines {
    fn dump(&self, writer: &mut Writer) {
        for line in self.0.lines() {
            writer.line(line);
        }
    }
}

/// Renders every inline formatting context in `store`, line by line.
fn inline_text(store: &zgui_layout::tree::store::LayoutStore) -> String {
    let mut out = String::new();
    let mut stack = vec![store.root().expect("a root")];
    let mut contexts = Vec::new();
    while let Some(key) = stack.pop() {
        if store.inline_resolution(key).is_some() {
            contexts.push(key);
        }
        stack.extend(store.node(key).children.iter().copied().rev());
    }
    for key in contexts {
        let resolution = store.inline_resolution(key).expect("a context");
        out.push_str(&format!(
            "context paragraph={} rtl={}\n",
            resolution.paragraph.index(),
            resolution.is_rtl
        ));
        for (index, line) in resolution.lines.iter().enumerate() {
            out.push_str(&format!(
                "  line {index} top={} height={} baseline={} offset={} width={} text={:?}\n",
                line.top,
                line.height(),
                line.baseline(),
                line.offset,
                line.width,
                line.text,
            ));
        }
        for placement in &resolution.placements {
            out.push_str(&format!(
                "  box line={} at=({}, {})\n",
                placement.line, placement.origin.0, placement.origin.1
            ));
        }
        for fragment in store.fragments_of_box(key) {
            let fragment = store.fragment(*fragment).expect("live");
            out.push_str(&format!(
                "  fragment {:?} at=({}, {}) size=({} x {})\n",
                fragment.kind,
                fragment.border_box.origin.x.0,
                fragment.border_box.origin.y.0,
                fragment.border_box.size.width.0,
                fragment.border_box.size.height.0,
            ));
        }
    }
    out
}

#[test]
fn inline_flow() {
    // Text, a nested inline box with its own font and its own edges, an image aligned three
    // different ways, and a paragraph that has to wrap: everything the context has to place at
    // once, in one golden a person can read.
    let natural = (48.0, 32.0);
    let fixture = Fixture::with_natural_size(
        Element::new("root").children(vec![Element::new("para").children(vec![
            Element::new("lead").text("alpha bravo "),
            Element::new("em").text("delta"),
            Element::new("mid").text(" gamma "),
            Element::new("picture").image(natural.0, natural.1),
            Element::new("tail").text(" kappa sigma omega"),
        ])]),
        "root { display: block; width: 240px }
         para { display: block; text-indent: 12px }
         em { display: inline; font-size: 24px; padding-left: 6px; margin-right: 4px }
         picture { display: inline; vertical-align: middle }",
        natural,
    );
    let mut store = fixture.box_tree();
    let mut content = support::measurer_with_images(natural.0, natural.1);
    lay_out(&mut store, &mut content, 240.0, 600.0);
    let rendering = inline_text(&store);
    assert_eq!(
        rendering,
        inline_text(&store),
        "the same tree renders the same bytes"
    );
    golden::assert_tree(&golden_path("inline-flow"), &InlineLines(rendering));
}

#[test]
fn floated_flow() {
    // A float 80 wide and 48 tall beside a paragraph: the two lines level with it start 80 in and
    // have 120 to fill, and every line below it has the whole 200 back. Nothing outside this
    // golden records the per-line bands, so a regression in the banding loop shows up here as
    // uniform offsets rather than as a failure anywhere else.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("side"),
            Element::new("para").text("alpha bravo delta gamma kappa sigma omega alpha bravo"),
        ]),
        "root { display: block; width: 200px }
         side { display: block; float: left; width: 80px; height: 48px }
         para { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 200.0, 600.0);
    golden::assert_tree(
        &golden_path("floated-flow"),
        &InlineLines(inline_text(&store)),
    );
}

#[test]
fn vertical_align_keywords() {
    // Every value `vertical-align` can take, on one image each, in one file. The assertions in
    // `tests/inline.rs` say what each number ought to be; this says what the whole set came out
    // at, so a change to any one of them is a diff a person reads rather than a number that moved.
    let natural = (60.0, 40.0);
    let mut out = String::new();
    for value in [
        "baseline",
        "sub",
        "super",
        "text-top",
        "text-bottom",
        "middle",
        "top",
        "bottom",
        "10px",
        "50%",
    ] {
        let css = format!(
            "root {{ display: block; width: 400px }}
             para {{ display: block; line-height: 120px }}
             picture {{ display: inline; vertical-align: {value} }}"
        );
        let fixture = Fixture::with_natural_size(
            Element::new("root").children(vec![Element::new("para").children(vec![
                Element::new("lead").text("one "),
                Element::new("picture").image(natural.0, natural.1),
            ])]),
            &css,
            natural,
        );
        let mut store = fixture.box_tree();
        let mut content = support::measurer_with_images(natural.0, natural.1);
        lay_out(&mut store, &mut content, 400.0, 600.0);
        out.push_str(&format!("vertical-align: {value}\n"));
        for line in inline_text(&store).lines() {
            out.push_str(&format!("  {line}\n"));
        }
    }
    golden::assert_tree(&golden_path("vertical-align"), &InlineLines(out));
}

#[test]
fn an_rtl_flex_row_lays_its_items_out_from_the_right() {
    // `flex-direction: row` in a right-to-left container puts the first item against the *right*
    // edge. taffy has no writing mode, so this was expected to need mirroring of our own before it
    // ever reached the engine; measured against taffy 0.12.2 it does not — the engine resolves the
    // main axis from `CoreStyle::direction`, which our style view answers from the cascaded
    // `direction`. The golden is here so that the day it stops being true is a diff rather than a
    // silence, and the explicit ordering below is here because a golden alone does not say which
    // of its numbers is the criterion.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("a"),
            Element::new("b"),
            Element::new("c"),
        ]),
        "root { display: flex; width: 300px; height: 40px; direction: rtl }
         a { display: block; width: 60px }
         b { display: block; width: 40px }
         c { display: block; width: 50px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 300.0, 200.0);

    let root = store.root().expect("a root");
    let items = store.node(root).children.clone();
    let edges: Vec<(f32, f32)> = items
        .iter()
        .map(|&item| {
            let layout = store.layout_of(item).expect("laid out");
            (layout.origin.x.0, layout.origin.x.0 + layout.size.width.0)
        })
        .collect();
    assert_eq!(
        edges,
        vec![(240.0, 300.0), (200.0, 240.0), (150.0, 200.0)],
        "the items ran left to right, so the container's direction reached the main axis"
    );
    golden::assert_tree(
        &golden_path("rtl-flex-row"),
        &BoxTree(zgui_layout::tree::print::to_text(&store)),
    );
}
