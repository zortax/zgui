//! Elements in different parts of the document that cascade to equal values share their groups.
//!
//! The engine's own sharing cache holds the last few elements of one tree level, so cousins far
//! apart in a list each get a style of their own. The style engine keeps the styles it made by
//! rule chain and gives an element with an equal result the groups of the kept one. These tests
//! pin that the sharing happens, and that what the frame draws does not change because of it.

mod support;

use zgui_view::{BuildCx, IntoView, NodeRef, View};

/// The sheet: two boxes of different widths, each holding a centred, wrapping paragraph.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .narrow { display: block; width: 55px }
                   .wide { display: block; width: 200px }
                   .label { display: block; text-align: center }";

/// The words of each paragraph: nine glyphs and two spaces.
const WORDS: &str = "abc abc abc";

/// The glyphs one paragraph of [`WORDS`] draws.
const GLYPHS: usize = 9;

/// The identities of the box group and the text group of the element `node` is bound to.
fn groups_of(window: &zgui_runtime::Window, node: NodeRef) -> (usize, usize) {
    let id = node.get_untracked().expect("the node reference is bound");
    let key = zgui_view_dom::id::to_document(id).expect("the handle names a document node");
    let document = window.document().borrow();
    let index = document
        .store()
        .index_of(key)
        .expect("the node is in the document");
    let style = document
        .node(index)
        .primary_style()
        .expect("the element is styled");
    (
        core::ptr::from_ref(style.get_box()) as usize,
        core::ptr::from_ref(style.get_inherited_text()) as usize,
    )
}

/// Two labels under parents of different widths share their groups, and each draws its own lines.
///
/// Each label keeps its own text group, so each keeps its own brush slot and its own shaped
/// paragraph: a shaped paragraph holds the lines of one width at a time, and the narrow label
/// draws three centred lines where the wide one draws one.
#[test]
fn equal_styles_under_different_parents_share_groups_and_draw_their_own_lines() {
    let narrow = NodeRef::new();
    let wide = NodeRef::new();
    let mut app = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::column().class("narrow").child(
                        zgui_elements::text()
                            .class("label")
                            .node_ref(narrow)
                            .child(WORDS),
                    ),
                )
                .child(
                    zgui_elements::column().class("wide").child(
                        zgui_elements::text()
                            .class("label")
                            .node_ref(wide)
                            .child(WORDS),
                    ),
                )
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);

    let window = &app.app().windows()[0];
    let (narrow_box, narrow_text) = groups_of(window, narrow);
    let (wide_box, wide_text) = groups_of(window, wide);
    assert_eq!(
        narrow_box, wide_box,
        "the two labels cascade to equal values under different parents and kept two box groups"
    );
    assert_ne!(
        narrow_text, wide_text,
        "the labels share one text group, so they share one brush slot and one shaped paragraph"
    );

    let mut lines = support::line_boxes(window);
    lines.sort_by(|a, b| a.size.width.0.total_cmp(&b.size.width.0));
    assert_eq!(
        lines.len(),
        4,
        "the narrow label wraps to three lines and the wide one fits on one: {lines:?}"
    );
    let sprites = &window.scene().primitives.mono_sprites;
    assert_eq!(sprites.len(), 2 * GLYPHS, "each label draws its own glyphs");
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
        let expected = if line.size.width.0 > 55.0 { GLYPHS } else { 3 };
        assert_eq!(
            inside, expected,
            "the line {line:?} holds {inside} glyphs, so a label drew the other label's lines"
        );
    }

    app.shut_down();
}
