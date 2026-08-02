//! What a text field *shows* while somebody types into it, read off the graphics device.
//!
//! # Why this is not next to the other field assertions
//!
//! Everything in `controls.rs` is a question about the tree: did the callback fire, does the element
//! hold the text, is it read-only. Every one of those can be true of a window in which the field on
//! the screen still says its placeholder, and that is not a hypothetical — it is the whole shape of
//! the defect this file was written for. The component kept its own text and its own caret, drew
//! them as a run of `<text>` boxes with a `<box>` between them, and the framework drew its own caret
//! from its own editing model over its own text: two carets, and a display that came from the model
//! the keystrokes were *not* going into.
//!
//! So nothing here is satisfied by the tree alone. Every assertion is made twice — once against the
//! document, which is what the field holds, and once against the display list, which is what the
//! window draws — and the second one is the one that would have caught it.
//!
//! # What "the display list" buys over a screenshot
//!
//! A caret is one device pixel wide. Two carets a pixel apart and one caret twice as wide are the
//! same photograph, and a picture of a field showing `Ada` and a picture of one showing a
//! placeholder differ only in ways a fixture would have to recognise letters to tell apart. The
//! display list has neither problem: a caret is a filled rectangle with a position and a width, and
//! a glyph names the atlas tile its coverage was rasterised into — so the letters a field drew can
//! be compared, one for one, against the letters a static run of the same text drew beside it.

mod desktop;
mod device;
mod painted;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::geom::{Device, DevicePx, Rect};
use zgui::prelude::{GetUntracked, Set, UnsyncCallback};
use zgui::reactive::RwSignal;
use zgui::view;
use zgui::view::{AnyView, NodeId, NodeRef};
use zgui::vocab::NamedKey;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::Stage;

/// The page every fixture is laid out on.
///
/// The reference line is styled to match a field's own text exactly — same family, same size, same
/// colour — because the whole of what it is for is producing the same glyphs from the same string.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 32px; gap: 24px; align-items: flex-start }
                     .field { width: 320px }
                     .reference {
                        font-family: var(--zui-type-family-sans);
                        font-size: var(--zui-type-size-sm);
                        line-height: var(--zui-type-leading-sm);
                        color: var(--zui-color-foreground);
                     }";

/// What is typed into every field below.
const TYPED: &str = "Ada";

/// Opens `view`, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($view:expr) => {
        match Stage::open(SHEET, $view) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// Where a fixture leaves the references it built, so a test can find the elements it made.
#[derive(Clone, Default)]
struct Built(Rc<RefCell<Vec<NodeRef>>>);

impl Built {
    /// Records the references this build produced.
    fn keep(&self, refs: &[NodeRef]) {
        *self.0.borrow_mut() = refs.to_vec();
    }

    /// The node the `which`th reference was bound to.
    ///
    /// # Panics
    ///
    /// Panics before the view has been built, and for an element that never bound its reference.
    fn node(&self, which: usize) -> NodeId {
        self.0.borrow()[which]
            .get_untracked()
            .expect("the element bound its reference when it was built")
    }
}

/// Everything the document says an element holds.
fn text_of(stage: &Stage, node: NodeId) -> String {
    stage
        .census()
        .nodes
        .iter()
        .find(|seen| seen.id == node)
        .map(|seen| seen.text.clone())
        .unwrap_or_default()
}

/// How wide a caret is, in device pixels, with room for the rounding either side of it.
///
/// One CSS pixel and never less than one device pixel is what the framework draws. Anything wider
/// than this inside a field is its border or its background, and anything this narrow is a caret.
const CARET_WIDTH: f32 = 3.0;

/// Every caret-shaped rectangle the last frame drew inside `node`.
///
/// Shaped rather than counted: a field draws a background and a border, both of which are filled
/// rectangles too, and the one thing that separates a caret from either is that it is a hair wide
/// and most of a line tall.
fn carets(stage: &Stage, node: NodeId) -> Vec<Rect<DevicePx, Device>> {
    stage
        .quads_in(stage.rect_of(node))
        .into_iter()
        .map(|quad| quad.bounds)
        .filter(|bounds| bounds.size.width.0 <= CARET_WIDTH && bounds.size.height.0 >= 8.0)
        .collect()
}

/// Which glyphs the last frame drew inside `node`, left to right.
fn spelling(stage: &Stage, node: NodeId) -> Vec<(u32, u32)> {
    stage
        .glyphs_in(stage.rect_of(node))
        .into_iter()
        .map(|glyph| glyph.tile)
        .collect()
}

// ---- an uncontrolled field --------------------------------------------------------------------

/// A field nobody owns the value of, and a line of static text saying what typing into it must
/// produce.
fn uncontrolled(built: &Built, seen: &Rc<RefCell<Vec<String>>>) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    let seen = Rc::clone(seen);
    move || {
        let field = NodeRef::new();
        let reference = NodeRef::new();
        let recorded = Rc::clone(&seen);
        built.keep(&[field, reference]);
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Input(
                        node_ref = field,
                        class = "field",
                        label = "Name",
                        placeholder = "Your name",
                        on_change = UnsyncCallback::new(move |text: String| {
                            recorded.borrow_mut().push(text);
                        })
                    )
                    text(node_ref = reference, class = "reference") {{TYPED}}
                }
            }
        })
    }
}

#[test]
fn typing_into_a_field_puts_one_caret_and_the_letters_typed_on_the_screen() {
    let built = Built::default();
    let seen: Rc<RefCell<Vec<String>>> = Rc::default();
    let mut stage = staged!(uncontrolled(&built, &seen));
    let field = built.node(0);
    let reference = built.node(1);

    // Nothing drawn by the field itself before the caret exists, and the placeholder is what is on
    // the screen: an empty field that drew glyphs of its own would be drawing the caret's text.
    assert!(
        carets(&stage, field).is_empty(),
        "an unfocused field is drawing a caret"
    );

    stage.click(stage.centre_of(field));
    stage.settle();
    stage.repaint();
    assert_eq!(
        carets(&stage, field).len(),
        1,
        "a focused field draws exactly one caret — two is the component drawing its own beside \
         the framework's"
    );
    let placeholder = spelling(&stage, field);
    assert!(
        !placeholder.is_empty(),
        "an empty field shows its placeholder"
    );

    stage.type_text(TYPED);
    stage.settle();
    stage.repaint();

    // What the field holds.
    assert_eq!(text_of(&stage, field), TYPED);
    assert_eq!(*seen.borrow(), ["A", "Ad", "Ada"], "and said so as it went");

    // What the window draws. One caret, still, and the letters that were typed — compared glyph by
    // glyph against the same string set beside it, so a field still showing its placeholder cannot
    // pass by drawing *something*.
    assert_eq!(
        carets(&stage, field).len(),
        1,
        "typing produced a second caret"
    );
    let drawn = spelling(&stage, field);
    assert_eq!(
        drawn,
        spelling(&stage, reference),
        "the field is not drawing the letters that were typed into it"
    );
    assert_ne!(
        drawn, placeholder,
        "the field is still drawing its placeholder"
    );

    // And backwards, which is the other half of a field being usable at all.
    stage.press_named(NamedKey::Backspace);
    stage.settle();
    stage.repaint();
    assert_eq!(text_of(&stage, field), "Ad");
    assert_eq!(carets(&stage, field).len(), 1);
    assert_eq!(
        spelling(&stage, field).len(),
        2,
        "a letter was taken out of the field and left on the screen"
    );
}

// ---- a field an application owns ---------------------------------------------------------------

/// A field driven by a signal, with the signal's own value shown beside it.
fn controlled(
    built: &Built,
    value: RwSignal<String, zgui::reactive::LocalStorage>,
) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    move || {
        let field = NodeRef::new();
        let reference = NodeRef::new();
        built.keep(&[field, reference]);
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Input(
                        node_ref = field,
                        class = "field",
                        label = "Name",
                        value = value,
                        on_change = UnsyncCallback::new(move |text: String| value.set(text))
                    )
                    text(node_ref = reference, class = "reference") {{TYPED}}
                }
            }
        })
    }
}

#[test]
fn a_field_its_application_owns_shows_what_is_typed_and_what_the_signal_is_set_to() {
    let value: RwSignal<String, zgui::reactive::LocalStorage> = RwSignal::new_local(String::new());
    let built = Built::default();
    let mut stage = staged!(controlled(&built, value));
    let field = built.node(0);
    let reference = built.node(1);

    stage.click(stage.centre_of(field));
    stage.type_text(TYPED);
    stage.settle();
    stage.repaint();

    // The loop closed: the keystroke was announced, the application wrote its signal, the signal
    // was written back into the field — and the field did not fight it.
    assert_eq!(value.get_untracked(), TYPED, "the signal never heard");
    assert_eq!(text_of(&stage, field), TYPED);
    assert_eq!(
        spelling(&stage, field),
        spelling(&stage, reference),
        "a controlled field is showing something other than what was typed into it"
    );
    assert_eq!(
        carets(&stage, field).len(),
        1,
        "the value written back took the caret away, or added a second one"
    );

    // And the other direction, which is the whole point of the caller owning it: the application
    // sets the value and the field follows.
    value.set("Grace".to_owned());
    stage.settle();
    stage.repaint();
    assert_eq!(text_of(&stage, field), "Grace");
    assert_eq!(
        spelling(&stage, field).len(),
        5,
        "the field is not showing the value its application set"
    );
    assert_eq!(
        carets(&stage, field).len(),
        1,
        "a field an application wrote to lost its caret"
    );
}

// ---- a field with nothing in it and nothing to show ---------------------------------------------

/// A field with no placeholder, which is the one that is genuinely empty when it is cleared.
///
/// A placeholder is drawn as generated content on the field itself, so a field that has one always
/// has something to lay out and always has a line to put a caret on. Without one, an emptied field
/// has no text at all — and that is the state the caret has to survive, because it is the state
/// somebody who has just selected everything and deleted it is looking at.
fn bare(
    built: &Built,
    value: RwSignal<String, zgui::reactive::LocalStorage>,
) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    move || {
        let field = NodeRef::new();
        built.keep(&[field]);
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Input(
                        node_ref = field,
                        class = "field",
                        label = "Name",
                        value = value,
                        on_change = UnsyncCallback::new(move |text: String| value.set(text))
                    )
                }
            }
        })
    }
}

/// How many output frames to watch the caret over: two blink periods, with room to spare.
const BLINKS: usize = 120;

#[test]
fn a_field_with_no_placeholder_still_shows_a_caret_once_it_is_emptied() {
    let value: RwSignal<String, zgui::reactive::LocalStorage> =
        RwSignal::new_local(TYPED.to_owned());
    let built = Built::default();
    let mut stage = staged!(bare(&built, value));
    let field = built.node(0);

    stage.click(stage.centre_of(field));
    stage.press_named(NamedKey::End);
    stage.settle();
    stage.repaint();
    assert_eq!(
        carets(&stage, field).len(),
        1,
        "a focused field with text in it draws no caret at all"
    );

    for _ in 0..TYPED.chars().count() {
        stage.press_named(NamedKey::Backspace);
    }
    stage.settle();
    stage.repaint();

    assert_eq!(text_of(&stage, field), "", "the field was not emptied");
    let cleared = carets(&stage, field);
    assert_eq!(
        cleared.len(),
        1,
        "a field somebody has just cleared shows nowhere for the next letter to go: {cleared:?}"
    );

    // And it is where the first letter would be drawn, not somewhere the last one used to be: a
    // caret left behind at the old end of the text is as wrong as one that is not there at all.
    let inside = stage.rect_of(field);
    assert!(
        cleared[0].origin.x.0 < inside.origin.x.0 + inside.size.width.0 / 2.0,
        "the caret of an emptied field is not at its start edge: {cleared:?} in {inside:?}"
    );

    // The half of the defect a repainted window hides. An emptied field has one line box of no
    // width, so the ink of the subtree under it is a rectangle no damage can intersect and the
    // whole of it is skipped: the frame that blinks the caret back on reaches nothing, and the
    // field stays blank until something else damages those pixels — which is what typing does, and
    // is why the caret came back the moment a letter went in. So the clock is run, without asking
    // for a full redraw, and the caret has to go off and come back.
    let mut phases: Vec<bool> = Vec::new();
    for _ in 0..BLINKS {
        stage.tick();
        phases.push(!carets(&stage, field).is_empty());
    }
    let went_off = phases
        .iter()
        .position(|drawn| !drawn)
        .unwrap_or_else(|| panic!("the caret never blinked off at all: {phases:?}"));
    assert!(
        phases[went_off..].iter().any(|drawn| *drawn),
        "the caret of an emptied field blinked off and never came back: {phases:?}"
    );
}
