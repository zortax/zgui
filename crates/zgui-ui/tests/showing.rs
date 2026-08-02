//! What a carousel and a select are *showing*, read off the frame the device drew.
//!
//! # Why this is not the composite assertions beside it
//!
//! Everything in `composites.rs` and `overlays.rs` asks the document a question: does the track
//! carry this custom property, does the trigger hold this text. Both of the defects these fixtures
//! were written for were true of a window in which every one of those answers was right.
//!
//! * A carousel translated its track by `-100%`, and a percentage translation resolves against the
//!   element being translated — so a track holding three slides moved three viewports on the first
//!   step and the whole strip left the frame. The custom property still went from `0` to `1`, which
//!   is all its assertion ever asked, so the suite was green over an empty viewport.
//! * A select showed its placeholder over a value it already had, because the text a value reads as
//!   lives on the option and the options are the list — which a closed select has not mounted. The
//!   fixture that covered it opened the list first, which is the one act that makes the defect go
//!   away.
//!
//! So nothing here reads the tree. Each fixture opens a real window on a real graphics device and
//! asks which letters were drawn and where they landed. A slide that is meant to have arrived is
//! compared against a reference line of the same words rendered in the same style beside it, so the
//! claim is that *those letters* are on the screen rather than that something is.

mod desktop;
mod device;
mod painted;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::geom::{Device, DevicePx, Rect};
use zgui::view;
use zgui::view::{AnyView, NodeId, NodeRef};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::painted::stage::{SETTLED, Stage};
use crate::painted::words::{assert_absent, assert_painted, found, spelling};

/// The page every fixture is laid out on.
///
/// The rail is narrower than any two slides put together, so exactly one slide is in view at a
/// time and a step that moved by anything other than one slide shows it.
///
/// The padding is [`MARGIN`] and not less. A carousel's arrows hang outside its own box, so a page
/// that puts one flush against its edge puts the back arrow off the window — which is a page that
/// left no room, not a carousel that lost its controls, and the two would otherwise read alike.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 64px; gap: 24px; align-items: flex-start }
                     .rail { width: 320px }
                     .plain { min-width: 0 }
                     .wide { flex: 0 0 400px }
                     .narrow { flex: 0 0 240px }";

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

/// Where the rail is in [`Built`], and where each reference line is.
const RAIL: usize = 0;

/// A three-slide carousel on a rail, with a line of the same words beside it for each slide.
///
/// The reference lines are outside the carousel and inherit exactly what the slides inherit, so a
/// line and the slide it stands for produce the same glyphs from the same string — which is the
/// whole of how a fixture reads what a viewport says without recognising letters.
fn gallery(built: &Built, widths: [&'static str; 3]) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    move || {
        let rail = NodeRef::new();
        let lines = [NodeRef::new(), NodeRef::new(), NodeRef::new()];
        built.keep(&[rail, lines[0], lines[1], lines[2]]);
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Carousel(node_ref = rail, class = "rail", label = "Photographs") {
                        CarouselContent {
                            CarouselItem(class = widths[0]) {text {"One"}}
                            CarouselItem(class = widths[1]) {text {"Two"}}
                            CarouselItem(class = widths[2]) {text {"Three"}}
                        }
                        CarouselPrevious()
                        CarouselNext()
                    }
                    text(node_ref = lines[0]) {"One"}
                    text(node_ref = lines[1]) {"Two"}
                    text(node_ref = lines[2]) {"Three"}
                }
            }
        })
    }
}

/// Whether `inner`'s middle falls inside `outer`.
fn within(outer: Rect<DevicePx, Device>, inner: Rect<DevicePx, Device>) -> bool {
    let x = inner.origin.x.0 + inner.size.width.0 / 2.0;
    let y = inner.origin.y.0 + inner.size.height.0 / 2.0;
    x >= outer.origin.x.0
        && x <= outer.origin.x.0 + outer.size.width.0
        && y >= outer.origin.y.0
        && y <= outer.origin.y.0 + outer.size.height.0
}

/// How far outside its own box a carousel's arrows may hang, in pixels.
///
/// They are placed in the margin either side of the strip rather than over it, so the rail's own
/// rectangle is the wrong place to look for them and its rectangle grown by this is the right one.
const MARGIN: f32 = 64.0;

/// The carousel's two arrows, left to right, which is back and then on.
///
/// Found by shape rather than by name, because that is what asserts they are there at all: they are
/// the two largest wordless boxes in the rail's neighbourhood, and a carousel whose strip pushed
/// them past its own edge has none.
///
/// Being *found* is not being **on the screen**, and the second is the claim worth making — an
/// arrow placed in a margin the page did not leave has a perfectly good rectangle a long way off
/// the left of the window, and nothing about the document says so. So each is checked against the
/// surface it would have to be drawn on.
///
/// # Panics
///
/// Panics when the neighbourhood does not hold two of them, which is a carousel with nothing to
/// press, and when either lands off the surface, which is one that cannot be pressed.
fn arrows(stage: &Stage, rail: NodeId) -> [NodeId; 2] {
    let rect = stage.rect_of(rail);
    let bounds = Rect::new(
        zgui::geom::Point::new(
            DevicePx(rect.origin.x.0 - MARGIN),
            DevicePx(rect.origin.y.0 - MARGIN),
        ),
        zgui::geom::Size::new(
            DevicePx(rect.size.width.0 + MARGIN * 2.0),
            DevicePx(rect.size.height.0 + MARGIN * 2.0),
        ),
    );
    let mut boxes: Vec<(NodeId, Rect<DevicePx, Device>)> = stage
        .census()
        .nodes
        .iter()
        .filter(|seen| seen.text.is_empty() && seen.area() > 0.0)
        .filter_map(|seen| seen.rect.map(|rect| (seen.id, rect)))
        .filter(|(_, rect)| within(bounds, *rect))
        .collect();
    boxes.sort_by(|left, right| {
        let area = |rect: Rect<DevicePx, Device>| rect.size.width.0 * rect.size.height.0;
        area(right.1).total_cmp(&area(left.1))
    });
    assert!(
        boxes.len() >= 2,
        "the rail holds {} wordless boxes, so its arrows are not on the screen",
        boxes.len()
    );
    boxes.truncate(2);
    boxes.sort_by(|left, right| left.1.origin.x.0.total_cmp(&right.1.origin.x.0));
    for (_, rect) in &boxes {
        assert!(
            rect.origin.x.0 >= 0.0
                && rect.origin.y.0 >= 0.0
                && rect.origin.x.0 + rect.size.width.0 <= crate::painted::stage::WIDTH
                && rect.origin.y.0 + rect.size.height.0 <= crate::painted::stage::HEIGHT,
            "an arrow was laid out at {rect:?}, which is off the window — a carousel's arrows hang \
             outside its own box, and this page left them nowhere to hang"
        );
    }
    [boxes[0].0, boxes[1].0]
}

/// How many elements say exactly `text`, laid out or not.
///
/// What tells one mounting from two. An option written once produces two of these — the option's
/// own element and the text node in it — so a list mounted twice over produces four, whether or
/// not the second copy was ever given a box.
fn counted(stage: &Stage, text: &str) -> usize {
    stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text == text)
        .count()
}

/// How many elements saying exactly `text` were laid out.
fn laid_out(stage: &Stage, text: &str) -> usize {
    stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text == text && node.area() > 0.0)
        .count()
}

/// Where each slide's words were drawn inside the rail, or nothing where they were not drawn.
///
/// One reading per slide, in the order the slides were written, each against its own reference
/// line — so `[None, Some(x), None]` is a viewport showing the second slide and nothing else.
///
/// The clock is run on and the window is made to draw itself whole before anything is read. Both
/// are load-bearing: the step is a transition, so a picture taken as it starts is a picture of the
/// slide that is leaving; and a window repaints what it damaged, so the most recent frame with
/// anything in it holds one control and a reading taken from it would find no reference line
/// anywhere and call every slide absent.
fn showing(stage: &mut Stage, built: &Built) -> [Option<f32>; 3] {
    stage.wait(SETTLED);
    let rail = stage.rect_of(built.node(RAIL));
    let mut seen = [None; 3];
    for (at, place) in seen.iter_mut().enumerate() {
        let word = spelling(stage, stage.rect_of(built.node(at + 1)));
        *place = found(stage, rail, &word);
    }
    seen
}

// ---- a carousel's slides ----------------------------------------------------------------------

#[test]
fn one_step_of_a_carousel_puts_the_next_slide_where_the_last_one_was() {
    // The whole of the geometry, in one reading: the second slide has to land exactly where the
    // first was, because that is what "one step is one slide" means. A carousel that divided its
    // track by the number of slides lands it somewhere else, and a carousel that translated by a
    // percentage of the track does not land it on the screen at all.
    let built = Built::default();
    let mut stage = staged!(gallery(&built, ["plain", "plain", "plain"]));

    let first = showing(&mut stage, &built);
    let before = first[0].expect("the first slide's words are on the screen from a clean start");
    assert_eq!(first[1], None, "and the second slide's are not");
    assert_eq!(first[2], None, "nor the third's");

    let [_, on] = arrows(&stage, built.node(RAIL));
    stage.click(stage.centre_of(on));
    stage.wait(SETTLED);

    let after = showing(&mut stage, &built);
    assert_eq!(after[0], None, "the first slide has left the viewport");
    assert_eq!(
        after[2], None,
        "and the third has not arrived with the second"
    );
    let now = after[1].expect("the second slide's words are on the screen after one step");
    assert!(
        (now - before).abs() < 1.0,
        "one step moved the strip by something other than one slide: the second slide's words \
         landed at {now}, where the first slide's were at {before}"
    );
}

#[test]
fn a_carousel_steps_one_slide_when_its_slides_are_different_widths() {
    // A step measured off the slides rather than divided out of the track. With a wide slide, a
    // narrow one and a wide one, every fraction of the track is the wrong distance for at least
    // one of the three.
    let built = Built::default();
    let mut stage = staged!(gallery(&built, ["wide", "narrow", "wide"]));
    let start = showing(&mut stage, &built)[0].expect("the first slide is showing to begin with");

    let [_, on] = arrows(&stage, built.node(RAIL));
    stage.click(stage.centre_of(on));
    stage.wait(SETTLED);
    let second = showing(&mut stage, &built);
    let at = second[1].expect("the narrow slide arrived");
    assert!(
        (at - start).abs() < 1.0,
        "the narrow slide landed at {at} rather than at {start}"
    );
    assert_eq!(second[0], None, "and the wide slide behind it is gone");

    stage.click(stage.centre_of(on));
    stage.wait(SETTLED);
    let third = showing(&mut stage, &built);
    let at = third[2].expect("the last slide arrived");
    assert!(
        (at - start).abs() < 1.0,
        "stepping off a narrow slide landed the next one at {at} rather than at {start}"
    );
    assert_eq!(third[1], None, "and the narrow slide is gone");
}

#[test]
fn a_carousel_keeps_its_arrows_on_the_screen_and_stops_at_the_last_slide() {
    let built = Built::default();
    let mut stage = staged!(gallery(&built, ["plain", "plain", "plain"]));
    let rail = built.node(RAIL);

    // Both arrows are on the window from the start — which `arrows` is what checks, and which the
    // strip being three viewports long is exactly what would prevent. They straddle the rail's own
    // edges rather than sitting inside it, so that is the pair asserted on: the back arrow is left
    // of where the strip starts and the forward one is right of where it ends.
    let [back, on] = arrows(&stage, rail);
    let strip = stage.rect_of(rail);
    assert!(
        stage.rect_of(back).origin.x.0 < strip.origin.x.0,
        "the back arrow belongs in the margin before the strip"
    );
    assert!(
        stage.rect_of(on).origin.x.0 > strip.origin.x.0 + strip.size.width.0 / 2.0,
        "and the forward one in the margin after it"
    );

    for _ in 0..4 {
        stage.click(stage.centre_of(on));
        stage.wait(SETTLED);
    }
    let end = showing(&mut stage, &built);
    assert!(
        end[2].is_some(),
        "a carousel that does not wrap stays on its last slide"
    );
    assert_eq!(end[0], None);
    assert_eq!(end[1], None);
    assert_eq!(
        arrows(&stage, rail).len(),
        2,
        "and its arrows are still drawn there"
    );

    for _ in 0..4 {
        stage.click(stage.centre_of(back));
        stage.wait(SETTLED);
    }
    let home = showing(&mut stage, &built);
    assert!(home[0].is_some(), "and back to the first, once");
    assert_eq!(home[1], None);
}

#[test]
fn the_arrow_keys_step_a_carousel_to_the_slide_they_paint() {
    let built = Built::default();
    let mut stage = staged!(gallery(&built, ["plain", "plain", "plain"]));
    let [_, on] = arrows(&stage, built.node(RAIL));

    // Focus has to be inside the carousel for its own axis' keys to reach it at all, so it is put
    // on the arrow that a person would have tabbed to.
    stage.click(stage.centre_of(on));
    stage.wait(SETTLED);
    stage.handles().host.focus(on);
    stage.settle();
    stage.press_named(zgui::vocab::NamedKey::ArrowLeft);
    stage.wait(SETTLED);

    let seen = showing(&mut stage, &built);
    assert!(seen[0].is_some(), "the left arrow key went back one slide");
    assert_eq!(seen[1], None);
}

// ---- a select's chosen option -----------------------------------------------------------------

#[test]
fn a_closed_select_paints_its_chosen_option_before_its_list_has_ever_opened() {
    // Nothing is pressed, so nothing has mounted the options. This is the whole of the fixture:
    // opening the list first is what the assertion this replaces did, and it is the one act that
    // makes the defect go away.
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Select(default_value = "gbp") {
                    SelectTrigger(a11y:label = "Currency") {
                        SelectValue(placeholder = "Choose one")
                    }
                    SelectContent {
                        SelectItem(value = "gbp") {"Pound sterling"}
                        SelectItem(value = "eur") {"Euro"}
                    }
                }
            }
        }
    }));
    // The clock is run on and the window redrawn whole, and nothing else: no press, no key, and
    // above all no opening of the list, which is the one act that would make this pass anyway.
    stage.wait(SETTLED);

    assert_painted(&stage, "Pound sterling");
    assert_absent(&stage, "Choose one");
    assert_absent(&stage, "Euro");
}

#[test]
fn opening_a_select_paints_every_option_once_and_no_option_twice() {
    // The half of the fix that is easy to break and hard to see: while the list is closed the
    // options are built out of sight, and the two mountings must never be in the document
    // together. A second copy would be a list twice as long as it looks — twice as far to walk
    // with the arrow keys, and every heading met twice by a reader.
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Select(default_value = "gbp") {
                    SelectTrigger(a11y:label = "Currency") {
                        SelectValue(placeholder = "Choose one")
                    }
                    SelectContent {
                        SelectGroup {
                            SelectLabel {"Europe"}
                            SelectItem(value = "gbp") {"Pound sterling"}
                            SelectItem(value = "eur") {"Euro"}
                        }
                        SelectGroup {
                            SelectLabel {"Elsewhere"}
                            SelectItem(value = "usd") {"US dollar"}
                        }
                    }
                }
            }
        }
    }));
    stage.wait(SETTLED);
    assert_eq!(
        laid_out(&stage, "Euro"),
        0,
        "a closed select puts none of its options on the screen"
    );

    stage.click(crate::painted::words::aim(&stage, "Pound sterling"));
    stage.wait(SETTLED);

    for word in ["Euro", "US dollar"] {
        assert_painted(&stage, word);
        assert_eq!(
            counted(&stage, word),
            2,
            "{word} is written once, so it is the option's own box and the text in it and nothing \
             else — a second mounting would double both"
        );
    }
    for heading in ["Europe", "Elsewhere"] {
        assert_eq!(
            counted(&stage, heading),
            2,
            "{heading} heads one group, once"
        );
    }
}

#[test]
fn choosing_from_a_select_paints_the_new_choice_on_the_closed_trigger() {
    let mut stage = staged!(|| AnyView::new(view! {
        ThemeProvider {
            column(class = "page") {
                Select(default_value = "gbp") {
                    SelectTrigger(a11y:label = "Currency") {
                        SelectValue(placeholder = "Choose one")
                    }
                    SelectContent {
                        SelectItem(value = "gbp") {"Pound sterling"}
                        SelectItem(value = "eur") {"Euro"}
                    }
                }
            }
        }
    }));

    stage.click(crate::painted::words::aim(&stage, "Pound sterling"));
    stage.wait(SETTLED);
    stage.click(crate::painted::words::aim(&stage, "Euro"));
    stage.wait(SETTLED);

    assert_painted(&stage, "Euro");
    assert_absent(&stage, "Pound sterling");
}
