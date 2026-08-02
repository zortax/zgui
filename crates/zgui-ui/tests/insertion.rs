//! Where a letter typed into a field that already has text in it actually lands.
//!
//! # Why an empty field proves nothing
//!
//! Every assertion about typing that starts from a blank field agrees with a field whose offsets
//! are shifted by a constant, because at offset zero every shift is the same shift. The defect this
//! file exists for was exactly that: a click placed the caret against the letter it was aimed at,
//! the letter typed went in three bytes further along, and the caret was then painted against the
//! *new* text three bytes back — so the two errors cancelled on the screen and a field that typed
//! into the middle of a word looked like a field with a lagging caret.
//!
//! So every fixture here starts with text in it, clicks at a boundary named by the glyphs that were
//! actually drawn, and asserts twice: the string the field holds afterwards, exactly, and where the
//! caret was painted relative to the glyphs either side of it.
//!
//! # Why the strings have no spaces in them
//!
//! A space draws no glyph, so a fixture that counted glyphs to find the fourth character would
//! count the wrong one. The strings below are unbroken so that the `n`th glyph is the `n`th
//! character, and the one case that needs a space — the multi-line field — locates its boundary
//! from the end instead.

mod desktop;
mod device;
mod painted;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::prelude::*;
use zgui::view::{AnyView, NodeId, NodeRef};
use zgui::vocab::NamedKey;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::Stage;

/// The page every fixture is laid out on.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 32px; gap: 24px; align-items: flex-start }
                     .field { width: 420px }";

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

/// Where a fixture leaves the reference it built.
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
    /// Panics before the view has been built.
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

/// Every glyph drawn inside `node`, left to right.
fn glyphs(stage: &Stage, node: NodeId) -> Vec<Rect<DevicePx, Device>> {
    stage
        .glyphs_in(stage.rect_of(node))
        .into_iter()
        .map(|glyph| glyph.bounds)
        .collect()
}

/// The one caret-shaped rectangle drawn inside `node`.
///
/// Caret-shaped rather than counted: a field draws a background and a border, both of which are
/// filled rectangles too, and what separates a caret from either is that it is a hair wide and most
/// of a line tall.
///
/// # Panics
///
/// Panics unless there is exactly one, because a fixture that reads the first of two carets is
/// asserting about a picture nobody would call correct.
fn caret(stage: &Stage, node: NodeId) -> Rect<DevicePx, Device> {
    let found: Vec<Rect<DevicePx, Device>> = stage
        .quads_in(stage.rect_of(node))
        .into_iter()
        .map(|quad| quad.bounds)
        .filter(|bounds| bounds.size.width.0 <= 3.0 && bounds.size.height.0 >= 8.0)
        .collect();
    assert_eq!(found.len(), 1, "a focused field draws exactly one caret");
    found[0]
}

/// A field holding `initial`, and nothing else on the page.
fn field(built: &Built, initial: &str) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    let initial = initial.to_owned();
    move || {
        let element = NodeRef::new();
        built.keep(&[element]);
        let initial = initial.clone();
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Input(node_ref = element, class = "field", label = "Name", default_value = initial)
                }
            }
        })
    }
}

/// A multi-line field holding `initial`.
fn textarea(built: &Built, initial: &str) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    let initial = initial.to_owned();
    move || {
        let element = NodeRef::new();
        built.keep(&[element]);
        let initial = initial.clone();
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Textarea(node_ref = element, class = "field", label = "Notes", default_value = initial)
                }
            }
        })
    }
}

/// The glyphs drawn inside `node`, grouped into the lines they were drawn on.
///
/// Grouped rather than sorted, because a glyph's box is as tall as the letter and not as tall as
/// the line: two letters on one line differ in where their boxes start by the height of an
/// ascender, and a fixture that read a list sorted by position alone would call that a second line
/// — or, worse, would read the first letter of the second line as the first letter of the first.
fn rows(stage: &Stage, node: NodeId) -> Vec<Vec<Rect<DevicePx, Device>>> {
    /// How far apart two glyph boxes have to start to be on different lines.
    ///
    /// Comfortably more than an ascender and comfortably less than a line.
    const APART: f32 = 10.0;

    let mut found = glyphs(stage, node);
    found.sort_by(|left, right| left.origin.y.0.total_cmp(&right.origin.y.0));
    let mut rows: Vec<Vec<Rect<DevicePx, Device>>> = Vec::new();
    for glyph in found {
        match rows.last_mut() {
            Some(row) if glyph.origin.y.0 - row[0].origin.y.0 < APART => row.push(glyph),
            _ => rows.push(vec![glyph]),
        }
    }
    for row in &mut rows {
        row.sort_by(|left, right| left.origin.x.0.total_cmp(&right.origin.x.0));
    }
    rows
}

/// Clicks in the leading half of the `which`th glyph of one line.
fn click_before_on(stage: &mut Stage, node: NodeId, row: usize, which: usize) {
    let target = rows(stage, node)[row][which];
    let at = Point::new(
        DevicePx(target.origin.x.0 + 1.0),
        DevicePx(target.origin.y.0 + target.size.height.0 / 2.0),
    );
    stage.click(at);
    stage.settle();
    stage.repaint();
}

/// Clicks in the leading half of the `which`th glyph, which puts the caret in front of it.
///
/// A hit resolves to the near edge of the cluster it lands in, so a point a shade inside a glyph's
/// leading edge is the boundary before that glyph and nothing else.
fn click_before(stage: &mut Stage, node: NodeId, which: usize) {
    let boxes = glyphs(stage, node);
    let target = boxes[which];
    let at = Point::new(
        DevicePx(target.origin.x.0 + 1.0),
        DevicePx(target.origin.y.0 + target.size.height.0 / 2.0),
    );
    stage.click(at);
    stage.settle();
    stage.repaint();
}

/// Clicks at `x`, on the line the glyphs are on.
fn click_at(stage: &mut Stage, node: NodeId, x: f32) {
    let boxes = glyphs(stage, node);
    let y = boxes[0].origin.y.0 + boxes[0].size.height.0 / 2.0;
    stage.click(Point::new(DevicePx(x), DevicePx(y)));
    stage.settle();
    stage.repaint();
}

/// Asserts the caret sits between two adjacent glyphs.
///
/// The claim a caret has to satisfy is not an exact coordinate — a glyph's box carries its side
/// bearings — but that it separates the two letters it is between, which is the whole of what a
/// person reads it as.
fn caret_between(stage: &Stage, node: NodeId, left: usize, right: usize) {
    caret_between_on(stage, node, 0, left, right);
}

/// Asserts the caret sits between two adjacent glyphs of one line.
fn caret_between_on(stage: &Stage, node: NodeId, row: usize, left: usize, right: usize) {
    let boxes = &rows(stage, node)[row];
    let caret = caret(stage, node);
    let after = boxes[left];
    let before = boxes[right];
    assert!(
        caret.origin.x.0 > after.origin.x.0 && caret.origin.x.0 <= before.origin.x.0 + 2.0,
        "the caret is at {:.1}, which is not between the glyphs at {:.1} and {:.1}",
        caret.origin.x.0,
        after.origin.x.0,
        before.origin.x.0
    );
}

#[test]
fn a_letter_typed_into_a_populated_field_lands_at_the_caret() {
    // The whole defect in one fixture: click in front of the fourth letter of a word, type, and
    // the letter has to be the fourth letter. An offset that counts something the document does not
    // hold — a directional control the shaper prefixed, a break between two text nodes — puts it a
    // fixed distance further along, and the caret that is painted afterwards hides it by being
    // wrong by the same amount in the other direction.
    let built = Built::default();
    let mut stage = staged!(field(&built, "AdaLovelace"));
    let element = built.node(0);

    click_before(&mut stage, element, 3);
    stage.type_text("X");
    stage.settle();
    stage.repaint();

    assert_eq!(text_of(&stage, element), "AdaXLovelace");
    // Painted between the letter typed and the one it was typed in front of.
    caret_between(&stage, element, 3, 4);
}

#[test]
fn typing_at_the_very_start_of_a_populated_field_goes_in_front_of_everything() {
    // Offset zero is where every shifted mapping agrees with the correct one, so this is the case
    // that used to pass: it is here to keep passing beside the one above rather than instead of it.
    let built = Built::default();
    let mut stage = staged!(field(&built, "AdaLovelace"));
    let element = built.node(0);

    let first = glyphs(&stage, element)[0].origin.x.0;
    click_at(&mut stage, element, first - 2.0);
    stage.type_text("12345");
    stage.settle();
    stage.repaint();

    assert_eq!(text_of(&stage, element), "12345AdaLovelace");
    caret_between(&stage, element, 4, 5);
}

#[test]
fn a_field_of_accented_letters_types_where_it_is_clicked() {
    // Two bytes per letter, so a mapping that counts characters where it should count bytes — or
    // the reverse — lands in a different place here than it does in ASCII, and an offset inside a
    // letter is a panic rather than a wrong answer.
    let built = Built::default();
    let mut stage = staged!(field(&built, "éàüñö"));
    let element = built.node(0);

    click_before(&mut stage, element, 2);
    stage.type_text("z");
    stage.settle();
    stage.repaint();

    assert_eq!(text_of(&stage, element), "éàzüñö");
    caret_between(&stage, element, 2, 3);
}

#[test]
fn a_field_of_cjk_types_where_it_is_clicked() {
    // Three bytes per character, and each is as wide as two Latin ones: a hit test that answered in
    // characters rather than bytes lands a third of the way along the string.
    let built = Built::default();
    let mut stage = staged!(field(&built, "日本語版"));
    let element = built.node(0);

    click_before(&mut stage, element, 2);
    stage.type_text("z");
    stage.settle();
    stage.repaint();

    assert_eq!(text_of(&stage, element), "日本z語版");
    caret_between(&stage, element, 2, 3);
}

#[test]
fn a_field_holding_an_emoji_types_after_it_rather_than_inside_it() {
    // Four bytes and, depending on the face, no coverage glyph at all — so the boundary is found by
    // counting back from the end, and what is asserted is that the letter goes in front of the
    // character that was clicked and not into the middle of the one before it.
    let built = Built::default();
    let mut stage = staged!(field(&built, "ab😀cd"));
    let element = built.node(0);

    let drawn = glyphs(&stage, element).len();
    click_before(&mut stage, element, drawn - 2);
    stage.type_text("z");
    stage.settle();
    stage.repaint();

    assert_eq!(text_of(&stage, element), "ab😀zcd");
}

#[test]
fn a_click_past_the_right_of_right_to_left_text_types_at_the_start_of_the_string() {
    // Written right to left, so the first character of the string is the rightmost glyph. A hit
    // test that resolved a point through the byte order rather than through the shaping would
    // answer this with the end of the string instead.
    let built = Built::default();
    let mut stage = staged!(field(&built, "שלום"));
    let element = built.node(0);

    let right = glyphs(&stage, element)
        .iter()
        .map(|glyph| glyph.origin.x.0 + glyph.size.width.0)
        .fold(f32::MIN, f32::max);
    click_at(&mut stage, element, right + 2.0);
    stage.type_text("X");
    stage.settle();
    stage.repaint();

    assert_eq!(text_of(&stage, element), "Xשלום");
}

#[test]
fn a_click_past_the_left_of_right_to_left_text_types_at_the_end_of_the_string() {
    // The other end of the same run, and the one that says the two answers are not simply swapped:
    // the leftmost glyph is the last character of the string, so a letter typed in front of it goes
    // after every character the field held.
    let built = Built::default();
    let mut stage = staged!(field(&built, "שלום"));
    let element = built.node(0);

    let left = glyphs(&stage, element)
        .iter()
        .map(|glyph| glyph.origin.x.0)
        .fold(f32::MAX, f32::min);
    click_at(&mut stage, element, left - 2.0);
    stage.type_text("Y");
    stage.settle();
    stage.repaint();

    assert_eq!(text_of(&stage, element), "שלוםY");
}

#[test]
fn enter_in_a_multi_line_field_opens_a_line_on_the_screen() {
    // Not only in the string. The paragraphs of an editor are separate text nodes, and text nodes
    // with nothing between them are laid out as one continuous run — so a break that only reached
    // the model would leave the second line drawn beside the first.
    let built = Built::default();
    let mut stage = staged!(textarea(&built, "abcd"));
    let element = built.node(0);
    assert_eq!(rows(&stage, element).len(), 1, "one line to start with");

    click_before(&mut stage, element, 2);
    stage.press_named(NamedKey::Enter);
    stage.settle();
    stage.repaint();

    assert_eq!(text_of(&stage, element), "ab\ncd");
    let lines = rows(&stage, element);
    assert_eq!(
        lines.len(),
        2,
        "the break was typed and nothing was drawn on a second line"
    );
    assert_eq!(
        lines[0].len(),
        2,
        "the first line kept what was in front of the caret"
    );
    assert_eq!(
        lines[1].len(),
        2,
        "and the second line took what was after it"
    );

    // The caret went with the text it split off, which is the start of the new line.
    let caret = caret(&stage, element);
    assert!(
        caret.origin.y.0 > lines[0][0].origin.y.0 + 4.0
            && caret.origin.x.0 <= lines[1][0].origin.x.0 + 2.0,
        "the caret is at ({:.1},{:.1}), not in front of the second line",
        caret.origin.x.0,
        caret.origin.y.0
    );
}

#[test]
fn a_single_line_field_still_refuses_enter() {
    // The other half of the same rule: Enter in a form submits it, so a single-line field must
    // leave the key alone rather than take a break nothing draws.
    let built = Built::default();
    let mut stage = staged!(field(&built, "abcd"));
    let element = built.node(0);

    click_before(&mut stage, element, 2);
    stage.press_named(NamedKey::Enter);
    stage.settle();
    stage.repaint();

    assert_eq!(text_of(&stage, element), "abcd");
    assert_eq!(glyphs(&stage, element).len(), 4);
}

#[test]
fn the_first_keystroke_in_a_multi_line_value_does_not_leave_the_old_text_beside_the_new() {
    // A view writes a value as one text node however many lines it has; the model writes it back as
    // one node per line. An element left in the first shape has its first keystroke write one line
    // into a node of its own and leave the node holding the whole original value where it was, so
    // the field shows its own first line twice — once as it was, once as it now is.
    let built = Built::default();
    let mut stage = staged!(textarea(&built, "onetwo\nthree"));
    let element = built.node(0);
    assert_eq!(text_of(&stage, element), "onetwo\nthree");
    assert_eq!(
        rows(&stage, element)
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        [6, 5],
        "the fixture is meant to draw its two lines once each"
    );

    click_before_on(&mut stage, element, 1, 3);
    stage.type_text("X");
    stage.settle();
    stage.repaint();

    assert_eq!(text_of(&stage, element), "onetwo\nthrXee");
    assert_eq!(
        rows(&stage, element)
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        [6, 6],
        "the field is drawing letters it does not hold, so a line is on the screen twice"
    );
    caret_between_on(&stage, element, 1, 3, 4);
}
