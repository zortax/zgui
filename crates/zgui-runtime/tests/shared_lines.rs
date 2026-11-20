//! Contexts with equal content at different widths each draw their own lines.
//!
//! Equal content shapes to one paragraph key, and a shaped paragraph holds the glyphs of one break
//! at a time. Labels that inherit every text property from one ancestor get one brush, so their
//! paragraphs are equal down to the key. Painting reads glyphs by that key, so each set of lines
//! that a context keeps has to have a form of its own. Contexts that keep equal lines share one.
//!
//! A frame draws again only the lines that changed, so a label that a hover leaves alone draws no
//! glyphs in that frame. Run with `ZGUI_INVARIANTS=1` to also check the form of such a label.

mod support;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// The sheet. Each hover changes the width of some labels, and leaves the others as they are.
///
/// The root sets the alignment, so every label inherits each text property from it.
const CSS: &str = "root { display: block; width: 600px; height: 300px; text-align: center }
                   .grid { display: grid; grid-template-columns: 55px 200px; width: 255px }
                   .grid:hover { grid-template-columns: 200px 55px }
                   .even { grid-template-columns: 200px 200px; width: 400px }
                   .even:hover { grid-template-columns: 55px 55px }
                   .wide { grid-template-columns: 200px 300px; width: 500px }
                   .wide:hover { grid-template-columns: 300px 200px }
                   .stay { display: block; width: 200px }
                   .pair { display: block; width: 200px; height: 200px }
                   .pair:hover { width: 55px }
                   .label { display: block }";

/// The words of each label: nine glyphs and two spaces.
const WORDS: &str = "abc abc abc";

/// The glyphs one label draws.
const GLYPHS: usize = 9;

/// A point inside the grid of a grid fixture.
const ON_GRID: (f32, f32) = (20.0, 10.0);

/// A point inside the narrowing container, below its labels.
///
/// Below them, because a label under the pointer takes `:hover`, and the frame that restyles it
/// draws nothing new.
const ON_PAIR: (f32, f32) = (20.0, 200.0);

/// A point outside every container.
const OUTSIDE: (f32, f32) = (590.0, 290.0);

fn pointer_at((x, y): (f32, f32)) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(y))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// The paragraph key the first inline context under `node` draws its glyphs from.
fn paragraph_of(window: &zgui_runtime::Window, node: NodeRef) -> zgui_text::ParagraphKey {
    let layout = window.layout().borrow();
    let id = node.get_untracked().expect("the label is mounted");
    let key = zgui_view_dom::id::to_document(id).expect("a document node");
    let mut stack: Vec<zgui_layout::BoxKey> = layout.boxes_of(key).to_vec();
    while let Some(box_) = stack.pop() {
        if let Some(resolution) = layout.inline_resolution(box_) {
            return resolution.key;
        }
        stack.extend(layout.node(box_).children.iter().copied());
    }
    panic!("the label laid out no line");
}

/// Asserts that the labels lay out `expected_lines` lines, and that each line drawn in the last
/// frame holds the glyphs of its own label's break.
///
/// A line of 55px or less holds one word, and a wider one holds all three. With `drawn`, the last
/// frame drew that many labels in full.
fn assert_own_lines(app: &App, drawn: Option<usize>, expected_lines: usize) {
    let window = &app.app().windows()[0];
    let lines = support::line_boxes(window);
    assert_eq!(
        lines.len(),
        expected_lines,
        "the labels lay out {expected_lines} lines: {lines:?}"
    );
    let sprites = &window.scene().primitives.mono_sprites;
    if let Some(labels) = drawn {
        assert_eq!(
            sprites.len(),
            labels * GLYPHS,
            "each label draws its own glyphs"
        );
    }
    let mut found = 0;
    for line in &lines {
        let inside = sprites
            .iter()
            .filter(|sprite| {
                // The centre, because a glyph is snapped to whole pixels and a centred line box
                // is not.
                let ink = sprite.ink();
                let x = ink.origin.x.0 + ink.size.width.0 / 2.0;
                let y = ink.origin.y.0 + ink.size.height.0 / 2.0;
                (line.origin.x.0..=line.right().0).contains(&x)
                    && (line.origin.y.0..=line.bottom().0).contains(&y)
            })
            .count();
        found += inside;
        if drawn.is_none() && inside == 0 {
            continue;
        }
        let expected = if line.size.width.0 > 55.0 { GLYPHS } else { 3 };
        assert_eq!(
            inside, expected,
            "the line {line:?} holds {inside} glyphs, so a label drew another label's lines"
        );
    }
    assert_eq!(found, sprites.len(), "a glyph lies outside every line box");
}

type App = zgui_platform_headless::Harness<zgui_runtime::Runtime>;

/// The application: one container per group, each holding its count of labels, and the node
/// references of the labels in order.
fn labels(groups: &'static [(&'static str, usize)]) -> (App, Vec<NodeRef>) {
    let refs: Vec<NodeRef> = groups
        .iter()
        .flat_map(|&(_, count)| (0..count).map(|_| NodeRef::new()))
        .collect();
    let held = refs.clone();
    let app = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let mut refs = held.iter().copied();
        let mut root = zgui_elements::column().class("root");
        for &(class, count) in groups {
            let mut group = zgui_elements::column().class(class);
            for _ in 0..count {
                group = group.child(
                    zgui_elements::text()
                        .class("label")
                        .node_ref(refs.next().expect("one reference per label"))
                        .child(WORDS),
                );
            }
            root = root.child(group);
        }
        Box::new(root.into_view().build(cx))
    });
    (app, refs)
}

/// Moves the pointer to `at` and lets the frame settle.
fn hover(app: &mut App, at: (f32, f32)) {
    app.deliver_to_first(pointer_at(at));
    app.settle(8);
}

/// The paragraph keys of `nodes`.
fn keys(app: &App, nodes: &[NodeRef]) -> Vec<zgui_text::ParagraphKey> {
    let window = &app.app().windows()[0];
    nodes
        .iter()
        .map(|&node| paragraph_of(window, node))
        .collect()
}

/// How many contexts moved to a copy of their paragraph since the last reset.
///
/// The counters are process-wide, so each test of this binary holds
/// [`exclusive`](zgui_profile::counter::exclusive) for its whole run.
fn separated() -> u64 {
    zgui_profile::counter::snapshot().get(zgui_profile::Counter::TextFormsSeparated)
}

#[test]
fn equal_labels_in_columns_of_different_widths_draw_their_own_lines() {
    let _turn = zgui_profile::counter::exclusive();
    let (mut app, refs) = labels(&[("grid", 2)]);
    app.settle(8);
    assert_own_lines(&app, Some(2), 4);
    let keys = keys(&app, &refs);
    assert_ne!(
        keys[0], keys[1],
        "two labels that keep different lines draw from one shaped paragraph"
    );

    // Both labels laid out again, each at the width the other had.
    hover(&mut app, ON_GRID);
    assert_own_lines(&app, Some(2), 4);

    // And back.
    hover(&mut app, OUTSIDE);
    assert_own_lines(&app, Some(2), 4);
    app.shut_down();
}

#[test]
fn equal_labels_at_one_width_share_one_paragraph() {
    let _turn = zgui_profile::counter::exclusive();
    let (mut app, refs) = labels(&[("grid even", 2)]);
    app.settle(8);
    assert_own_lines(&app, Some(2), 2);
    let keys = keys(&app, &refs);
    assert_eq!(
        keys[0], keys[1],
        "two labels with equal content at one width hold two shaped paragraphs"
    );
    app.shut_down();
}

#[test]
fn equal_labels_resized_together_keep_one_paragraph() {
    let _turn = zgui_profile::counter::exclusive();
    let (mut app, refs) = labels(&[("grid even", 2)]);
    app.settle(8);
    zgui_profile::counter::reset();
    for (at, lines) in [(ON_GRID, 6), (OUTSIDE, 2), (ON_GRID, 6)] {
        hover(&mut app, at);
        assert_own_lines(&app, Some(2), lines);
        let keys = keys(&app, &refs);
        assert_eq!(
            keys[0], keys[1],
            "two labels that keep equal lines after a resize hold two shaped paragraphs"
        );
    }
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(separated(), 0, "a label moved to a copy of its paragraph");
    }
    app.shut_down();
}

#[test]
fn equal_single_lines_at_different_widths_share_one_paragraph() {
    let _turn = zgui_profile::counter::exclusive();
    // Centred, so the lines start at different offsets. The glyphs of a line are placed against
    // its line box, so one form serves both.
    let (mut app, refs) = labels(&[("grid wide", 2)]);
    app.settle(8);
    zgui_profile::counter::reset();
    for at in [None, Some(ON_GRID), Some(OUTSIDE)] {
        if let Some(at) = at {
            hover(&mut app, at);
        }
        assert_own_lines(&app, Some(2), 2);
        let keys = keys(&app, &refs);
        assert_eq!(
            keys[0], keys[1],
            "two labels with equal lines at different widths hold two shaped paragraphs"
        );
    }
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(separated(), 0, "a label moved to a copy of its paragraph");
    }
    app.shut_down();
}

#[test]
fn an_equal_label_left_alone_keeps_its_lines() {
    let _turn = zgui_profile::counter::exclusive();
    // The hover narrows the second label only. The first is not laid out again, and the shared
    // form has to be put back on its lines.
    let (mut app, refs) = labels(&[("stay", 1), ("pair", 1)]);
    app.settle(8);
    assert_own_lines(&app, Some(2), 2);
    let rest = keys(&app, &refs);
    assert_eq!(rest[0], rest[1], "the fixture holds no shared paragraph");
    hover(&mut app, ON_PAIR);
    assert_own_lines(&app, None, 4);
    let hovered = keys(&app, &refs);
    assert_eq!(
        hovered[0], rest[0],
        "the label left alone moved off the shared paragraph"
    );
    assert_ne!(
        hovered[1], rest[0],
        "the narrowed label shares the other's form"
    );
    hover(&mut app, OUTSIDE);
    assert_own_lines(&app, None, 2);
    app.shut_down();
}

#[test]
fn a_third_equal_label_left_alone_keeps_its_lines() {
    let _turn = zgui_profile::counter::exclusive();
    // Two of three labels are narrowed together. Neither may record its lines for the
    // paragraph, because the third still keeps the old ones. The two share one copy.
    let (mut app, refs) = labels(&[("stay", 1), ("pair", 2)]);
    app.settle(8);
    assert_own_lines(&app, Some(3), 3);
    let rest = keys(&app, &refs);
    assert!(
        rest.iter().all(|&key| key == rest[0]),
        "the fixture holds no shared paragraph"
    );
    hover(&mut app, ON_PAIR);
    assert_own_lines(&app, None, 7);
    let hovered = keys(&app, &refs);
    assert_eq!(
        hovered[0], rest[0],
        "the label left alone moved off the shared paragraph"
    );
    assert_ne!(
        hovered[1], rest[0],
        "a narrowed label shares the third's form"
    );
    assert_eq!(
        hovered[1], hovered[2],
        "two narrowed labels that keep equal lines hold a copy each"
    );
    hover(&mut app, OUTSIDE);
    assert_own_lines(&app, None, 3);
    app.shut_down();
}
