//! Where a line that does not fit its box is cut off, and what marks the cut.

mod support;

use support::{Element, Fixture, lay_out, measurer};
use zgui_layout::tree::store::LayoutStore;

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

/// Lays out one nowrap paragraph `width` device pixels wide under `declarations`.
fn laid(text: &'static str, width: f32, declarations: &str) -> (LayoutStore, support::Content) {
    let css = format!(
        "root {{ display: block; width: 400px }}
         para {{ display: block; width: {width}px; white-space: nowrap; {declarations} }}",
    );
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("para").text(text)]),
        &css,
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 600.0);
    (store, content)
}

/// The first line's cut, if it has one.
fn cut(store: &LayoutStore) -> Option<zgui_layout::inline::ellipsis::LineEllipsis> {
    store.inline_resolution(inline_root(store))?.lines[0].ellipsis
}

#[test]
fn a_line_that_fits_is_never_cut() {
    // Six characters at eight pixels each in a box with room for eight of them, so nothing
    // overflows and nothing is shaped to mark a cut that did not happen.
    let (store, _) = laid(
        "abcdef",
        8.0 * ADVANCE,
        "overflow: hidden; text-overflow: ellipsis",
    );
    assert_eq!(cut(&store), None);
    let resolution = store
        .inline_resolution(inline_root(&store))
        .expect("laid out");
    assert!(
        resolution.ellipsis.is_empty(),
        "nothing was shaped to mark it"
    );
}

#[test]
fn a_line_that_overflows_is_cut_on_a_cluster_boundary() {
    // Ten characters in room for six. One of the six is spent on the ellipsis, so five survive.
    let (store, _) = laid(
        "abcdefghij",
        6.0 * ADVANCE,
        "overflow: hidden; text-overflow: ellipsis",
    );
    let cut = cut(&store).expect("the line was cut");
    assert!(!cut.at_start);
    assert_eq!(
        cut.cutoff,
        5.0 * ADVANCE,
        "on the boundary five clusters in"
    );

    let resolution = store
        .inline_resolution(inline_root(&store))
        .expect("laid out");
    let mark = resolution.ellipsis.end.expect("an end mark was shaped");
    assert_eq!(mark.width, ADVANCE, "one ellipsis character wide");
    assert!(
        resolution.ellipsis.start.is_none(),
        "no line was cut at its start"
    );
}

#[test]
fn a_box_that_does_not_clip_never_cuts_its_lines() {
    // The property applies to a box whose content was cut, and content that is allowed to spill out
    // of its box was never cut. Without this the ellipsis would appear over text that is visibly
    // still there beside it.
    let (store, _) = laid("abcdefghij", 6.0 * ADVANCE, "text-overflow: ellipsis");
    assert_eq!(cut(&store), None);
}

#[test]
fn clip_cuts_the_line_and_writes_nothing_where_it_cut() {
    // The initial value. The box's own overflow already stops the content being drawn, so there is
    // nothing to record and nothing to shape.
    let (store, _) = laid("abcdefghij", 6.0 * ADVANCE, "overflow: hidden");
    assert_eq!(cut(&store), None);
    let resolution = store
        .inline_resolution(inline_root(&store))
        .expect("laid out");
    assert!(resolution.ellipsis.is_empty());
}

#[test]
fn the_string_form_is_measured_as_the_string_it_names() {
    // `text-overflow: "..."` is three characters and reserves three characters' worth of room, so
    // two of the six fit rather than five.
    let (store, _) = laid(
        "abcdefghij",
        6.0 * ADVANCE,
        "overflow: hidden; text-overflow: \"...\"",
    );
    let resolution = store
        .inline_resolution(inline_root(&store))
        .expect("laid out");
    let mark = resolution.ellipsis.end.expect("an end mark");
    assert_eq!(mark.width, 3.0 * ADVANCE);
    assert_eq!(cut(&store).expect("cut").cutoff, 3.0 * ADVANCE);
}

#[test]
fn the_ellipsis_changes_no_geometry() {
    // The specification is explicit: the mark is drawn over the content and the lines keep the
    // widths they were broken at. A cut that moved an edge would be a box whose height depended on
    // whether its text happened to fit, which is the failure this design exists to avoid.
    let (plain, _) = laid("abcdefghij", 6.0 * ADVANCE, "overflow: hidden");
    let (marked, _) = laid(
        "abcdefghij",
        6.0 * ADVANCE,
        "overflow: hidden; text-overflow: ellipsis",
    );
    let line = |store: &LayoutStore| {
        let resolution = store
            .inline_resolution(inline_root(store))
            .expect("laid out");
        (
            resolution.lines[0].width,
            resolution.lines[0].offset,
            resolution.lines[0].height(),
            resolution.lines.len(),
        )
    };
    assert_eq!(line(&plain), line(&marked));
}

#[test]
fn a_mark_wider_than_the_box_leaves_nothing_of_the_line() {
    // Room for one character and a three-character mark: nothing survives, and the cut is at the
    // line's own start rather than at a negative coordinate.
    let (store, _) = laid(
        "abcdefghij",
        1.0 * ADVANCE,
        "overflow: hidden; text-overflow: \"...\"",
    );
    let cut = cut(&store).expect("the line was cut");
    assert_eq!(cut.cutoff, 0.0);
}

#[test]
fn a_cut_moves_the_fingerprint_the_fragment_pass_compares() {
    // A repaint is decided by comparing a fragment against the one it replaces, and a line box that
    // was cut is exactly the same rectangle as one that was not. Without the fingerprint, turning
    // `text-overflow` on would change what is drawn and nothing would be redrawn.
    let (plain, _) = laid("abcdefghij", 6.0 * ADVANCE, "overflow: hidden");
    let (marked, _) = laid(
        "abcdefghij",
        6.0 * ADVANCE,
        "overflow: hidden; text-overflow: ellipsis",
    );
    let hash = |store: &LayoutStore| {
        zgui_layout::inline::ellipsis::line_hash(
            &store
                .inline_resolution(inline_root(store))
                .expect("laid out")
                .lines[0],
        )
    };
    assert_eq!(hash(&plain), 0);
    assert_ne!(hash(&marked), 0);
}

/// A label that shrinks beside an icon, laid out at one width and then at a narrower one.
///
/// The shape of a row in a file tree or a thread list: an icon, then a nowrap label that takes
/// what is left and is cut off when that is not enough. The second layout is a second frame over
/// the same store, so every cache the first one filled is still there.
fn row_fixture(declarations: &str) -> Fixture {
    let css = format!(
        "root {{ display: flex; width: 100%; height: 40px }}
         icon {{ width: 16px; height: 16px; flex: 0 0 auto }}
         para {{ flex: 1 1 auto; min-width: 0; white-space: nowrap; overflow: hidden;
                 text-overflow: ellipsis; {declarations} }}
         mark {{ display: inline-block; vertical-align: top; width: 8px; height: 8px }}",
    );
    let label = if declarations.contains("with-mark") {
        Element::new("para")
            .text("abcdefghijklmnop")
            .children(vec![Element::new("mark")])
    } else {
        Element::new("para").text("abcdefghijklmnop")
    };
    Fixture::with_natural_size(
        Element::new("root").children(vec![Element::new("icon").image(16.0, 16.0), label]),
        &css,
        (16.0, 16.0),
    )
}

#[test]
fn narrowing_a_row_cuts_the_label_in_the_same_pass() {
    let fixture = row_fixture("");
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = support::Frame::new();
    // Sixteen characters at eight pixels: 128 of label, room for 224 beside the icon.
    support::relayout(&mut frame, &mut store, &mut content, 240.0, 600.0);
    assert_eq!(cut(&store), None, "the label fits at the wide width");

    // Room for ten characters beside the icon: the label is cut, and the cut is in the
    // resolution the very pass that narrowed it left behind.
    support::relayout(&mut frame, &mut store, &mut content, 96.0, 600.0);
    let cut = cut(&store).expect("the narrowed label was cut");
    assert!(!cut.at_start, "the cut is at the trailing edge: {cut:?}");
}

#[test]
fn a_probe_over_a_line_box_aligned_atomic_leaves_the_kept_cut_alone() {
    // A label holding an atomic aligned with the line box's own edges cannot answer a probe from
    // its recalled break, so the probe falls through to a full pass. That pass must not replace
    // what the kept pass left: the layout that follows it can be a cache hit, and the cut would
    // stay gone.
    let fixture = row_fixture("/* with-mark */");
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = support::Frame::new();
    support::relayout(&mut frame, &mut store, &mut content, 240.0, 600.0);
    support::relayout(&mut frame, &mut store, &mut content, 96.0, 600.0);
    assert!(cut(&store).is_some(), "the narrowed label was cut");

    // A re-run of the row at the same width: the label is probed at the container's width and
    // its final layout is answered from the cache.
    support::relayout(&mut frame, &mut store, &mut content, 96.0, 700.0);
    let cut = cut(&store).expect("the cut survived a pass that only probed the label");
    assert!(!cut.at_start, "{cut:?}");
}

/// The line fragment's stored fingerprint, which is what the painter keys its recording on.
fn stored_hash(store: &LayoutStore) -> u64 {
    let root = inline_root(store);
    store
        .fragments_of_box(root)
        .iter()
        .filter_map(|frag| store.fragment(*frag))
        .find(|fragment| {
            matches!(
                fragment.kind,
                zgui_layout::fragment::FragmentKind::Line { .. }
            )
        })
        .map(|fragment| fragment.content_hash)
        .expect("the label has a line fragment")
}

#[test]
fn the_fragment_keeps_the_fingerprint_of_its_cut() {
    // The fingerprint is compared by the fragment pass and read by the painter, and the two read
    // it from different places: the pass computes it from the lines, the painter takes it off the
    // fragment. A fragment that never stored it says "not cut" for ever, so a recording of the
    // cut line is replayed after the cut has gone — and a recording of the whole line after one
    // has appeared.
    let fixture = row_fixture("");
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = support::Frame::new();
    support::relayout(&mut frame, &mut store, &mut content, 240.0, 600.0);
    assert_eq!(
        stored_hash(&store),
        0,
        "an uncut line stores no fingerprint"
    );

    support::relayout(&mut frame, &mut store, &mut content, 96.0, 600.0);
    let expected = zgui_layout::inline::ellipsis::line_hash(
        &store
            .inline_resolution(inline_root(&store))
            .expect("laid out")
            .lines[0],
    );
    assert_ne!(expected, 0, "the narrowed line is cut");
    assert_eq!(
        stored_hash(&store),
        expected,
        "the fragment carries the cut the pass compared"
    );

    support::relayout(&mut frame, &mut store, &mut content, 240.0, 600.0);
    assert_eq!(
        stored_hash(&store),
        0,
        "widened again, the fragment says the line is whole"
    );
}
