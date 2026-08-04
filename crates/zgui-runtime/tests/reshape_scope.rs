//! What one element's brush moving costs the rest of the document.
//!
//! A shaped paragraph carries the brush slot its glyphs were shaped with, so an element that has to
//! leave a slot it was sharing has shaping that names the wrong brush and can only be corrected by
//! shaping its text again. That argument is about **one element**, and the whole of what these
//! tests hold down is that the invalidation it justifies is about one element too.
//!
//! The shape of the failure they exist for is not a wrong pixel. A window that answers it by
//! dropping every shaped paragraph and invalidating every box draws exactly the right frame — and
//! reflows the entire document to do it, on a keystroke, a focus move or one frame in the middle of
//! a scroll. So the assertions here are **counts**: how many paragraphs were thrown away and how
//! many boxes had their layout thrown away, which read the same on any machine and are the only
//! evidence that separates a scoped invalidation from a correct one.
//!
//! Four cases, and each is the other three's control:
//!
//! * moving focus, which restyles one element and moves no colour at all;
//! * one element of many leaving the slot they share, which is the case the mechanism exists for;
//! * every element leaving it at once, which is what says the mechanism was scoped rather than
//!   deleted;
//! * a theme change, which moves every colour in the window and is answered by writing through the
//!   slots with nothing re-shaped, because that is what the slots are for.

mod support;

use zgui_platform::SurfaceEvent;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, GetUntracked, Set};
use zgui_view::{AttrName, BuildCx, IntoView, View, ViewHost};
use zgui_vocab::{KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp};

/// Four strings that inherit one colour, and a focusable control that generates a fifth.
///
/// Everything holding text inherits the page's colour without declaring one, which is what puts
/// them all on a single brush slot — the arrangement a component library produces, and the one in
/// which any single element moving off the slot cannot be answered by rewriting it.
///
/// The focus rule deliberately moves nothing about text. It is the case the amplifier fired on: the
/// element re-cascades, its generated content is a fresh cascade result at a fresh address, and the
/// colour under it is the colour it already had.
const CSS: &str = ":root { display: block; width: 400px; height: 300px }
                   .page { display: block; color: rgb(10, 10, 10) }
                   text { display: block }
                   control { display: block; width: 100px; height: 24px }
                   control::before { content: \"xy\" }
                   control:focus { outline-color: rgb(0, 0, 255) }
                   .page[data-theme=\"dark\"] { color: rgb(210, 210, 210) }
                   text[data-hot=\"1\"] { color: rgb(120, 120, 120) }";

/// The grey every glyph in the window is drawn in, ordered down the window and then across it.
fn drawn_greys(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<u8> {
    let mut placed: Vec<_> = harness.app().windows()[0]
        .scene()
        .primitives
        .mono_sprites
        .iter()
        .map(|sprite| {
            (
                sprite.bounds[1].to_bits(),
                sprite.bounds[0].to_bits(),
                (sprite.color[0] * 255.0).round() as u8,
            )
        })
        .collect();
    placed.sort_unstable();
    placed.into_iter().map(|(_, _, grey)| grey).collect()
}

/// How many boxes the layout tree holds, which is the number a document-wide invalidation reaches.
fn boxes(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> usize {
    harness.app().windows()[0].layout().borrow().keys().len()
}

/// The page, its four strings and the control, with two attributes a test drives.
fn page(
    cx: &mut BuildCx<'_>,
    dark: RwSignal<bool>,
    hot: RwSignal<u8>,
) -> Box<dyn zgui_view::Anchor> {
    /// One string, hot when its own bit of `hot` is set.
    fn word(
        hot: RwSignal<u8>,
        bit: u8,
        text: &'static str,
    ) -> zgui_elements::Element<zgui_elements::Text> {
        zgui_elements::text()
            .attribute(AttrName::new("data-hot"), move || {
                (hot.get() & (1 << bit) != 0).then(|| "1".to_owned())
            })
            .child(text)
    }

    let view = zgui_elements::column()
        .class("page")
        .attribute(AttrName::new("data-theme"), move || {
            dark.get().then(|| "dark".to_owned())
        })
        .child(word(hot, 0, "aa"))
        .child(word(hot, 1, "bb"))
        .child(word(hot, 2, "cc"))
        .child(word(hot, 3, "dd"))
        .child(zgui_elements::control())
        .into_view();
    Box::new(view.build(cx)) as Box<dyn zgui_view::Anchor>
}

/// A settled window over the fixture, and the two signals that drive it.
fn opened() -> (
    RwSignal<bool>,
    RwSignal<u8>,
    zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) {
    let dark = RwSignal::new(false);
    let hot = RwSignal::new(0u8);
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| page(cx, dark, hot));
    harness.settle(16);
    (dark, hot, harness)
}

/// The same fixture with every text element sharing one paragraph cache key.
fn opened_with_identical_text() -> (
    RwSignal<u8>,
    zgui_platform_headless::Harness<zgui_runtime::Runtime>,
) {
    let hot = RwSignal::new(0u8);
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        fn word(hot: RwSignal<u8>, bit: u8) -> zgui_elements::Element<zgui_elements::Text> {
            zgui_elements::text()
                .attribute(AttrName::new("data-hot"), move || {
                    (hot.get() & (1 << bit) != 0).then(|| "1".to_owned())
                })
                .child("aa")
        }

        Box::new(
            zgui_elements::column()
                .class("page")
                .child(word(hot, 0))
                .child(word(hot, 1))
                .child(word(hot, 2))
                .child(word(hot, 3))
                .child(zgui_elements::control())
                .into_view()
                .build(cx),
        ) as Box<dyn zgui_view::Anchor>
    });
    harness.settle(16);
    (hot, harness)
}

/// Damages the whole window without moving anything in it, so that every glyph is drawn again.
///
/// The display list holds what the frame emitted, and a frame that repainted one label emitted one
/// label. Reading the colours of the whole window therefore needs a frame that was asked for all of
/// them — and asking for them by damage rather than by a resize is what keeps the question about
/// the brushes: nothing reflows, nothing is shaped again, and every glyph is drawn through the slot
/// it named when it was shaped.
fn repaint_everything(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>) {
    let surface = harness
        .platform()
        .offscreens()
        .first()
        .map(|surface| zgui_platform::Surface::id(surface.as_ref()))
        .expect("the application opened its window");
    harness.deliver(surface, SurfaceEvent::Occluded(true));
    harness.settle(8);
    harness.deliver(surface, SurfaceEvent::Occluded(false));
    harness.settle(8);
    harness.advance(std::time::Duration::from_millis(50));
    harness.settle(8);
}

/// Every glyph in the window, redrawn first so that the answer is about all of them.
fn every_grey(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<u8> {
    repaint_everything(harness);
    drawn_greys(harness)
}

/// What the window's counters did across `act`.
fn moved(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    act: impl FnOnce(&mut zgui_platform_headless::Harness<zgui_runtime::Runtime>),
) -> zgui_profile::Counters {
    let before = zgui_profile::counter::snapshot();
    act(harness);
    harness.settle(16);
    before.delta(&zgui_profile::counter::snapshot())
}

/// Asserts that nothing reached for the document-wide invalidation.
///
/// Both halves, because either one alone is still a whole-document cost: the paragraphs are what
/// has to be shaped again, and the boxes are what has to be measured and arranged again from them.
fn nothing_document_wide(moved: &zgui_profile::Counters, what: &str) {
    assert_eq!(
        moved.paragraphs_forgotten, 0,
        "{what} threw away every shaped paragraph in the window"
    );
    assert_eq!(
        moved.boxes_marked_all_dirty, 0,
        "{what} invalidated every box in the layout tree"
    );
}

#[test]
fn moving_focus_reshapes_no_paragraph() {
    let _turn = zgui_profile::counter::exclusive();
    let (_dark, _hot, mut harness) = opened();
    let opening = every_grey(&mut harness);
    assert!(
        opening.len() >= 10,
        "the fixture drew {} glyphs, so there is not enough text here for a document-wide \
         invalidation to be distinguishable from a scoped one",
        opening.len()
    );

    let counted = moved(&mut harness, |harness| {
        harness.deliver_to_first(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab)),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    });

    // The control. A Tab that moved no focus restyles nothing, and a test over it would report no
    // reshaping for the reason that nothing happened at all.
    assert!(
        harness.app().windows()[0]
            .host()
            .focused()
            .get_untracked()
            .is_some(),
        "the Tab press moved focus nowhere, so no element was restyled by it"
    );
    assert!(
        counted.elements_restyled + counted.elements_recascaded > 0,
        "focus landed without restyling anything"
    );

    nothing_document_wide(&counted, "moving focus");
    assert_eq!(
        counted.paragraphs_evicted, 0,
        "moving focus re-shaped text, and it changed no colour to justify it"
    );
    assert_eq!(
        counted.text_shaped, 0,
        "moving focus re-shaped text, and it changed no colour to justify it"
    );
    assert_eq!(
        every_grey(&mut harness),
        opening,
        "moving focus changed what colour the window's text is drawn in"
    );
}

#[test]
fn one_element_leaving_a_shared_slot_reshapes_its_own_text_and_no_other() {
    let _turn = zgui_profile::counter::exclusive();
    let (_dark, hot, mut harness) = opened();
    let held = boxes(&harness);

    let counted = moved(&mut harness, |_| hot.set(0b0001));

    nothing_document_wide(&counted, "one element leaving a shared slot");
    assert_eq!(
        counted.paragraphs_evicted, 1,
        "one element left the slot it was sharing and {} paragraphs were thrown away",
        counted.paragraphs_evicted
    );
    assert!(
        counted.boxes_reshaped > 0 && (counted.boxes_reshaped as usize) < held,
        "one element left the slot it was sharing and {} of the window's {held} boxes were \
         invalidated",
        counted.boxes_reshaped
    );
    assert_eq!(
        every_grey(&mut harness),
        [120, 120, 10, 10, 10, 10, 10, 10, 10, 10],
        "the element that left the slot is not drawn in its new colour, or one that stayed \
         followed it"
    );
}

#[test]
fn changing_one_of_two_identical_paragraphs_keeps_the_shared_old_shaping() {
    let _turn = zgui_profile::counter::exclusive();
    let (hot, mut harness) = opened_with_identical_text();

    let counted = moved(&mut harness, |_| hot.set(0b0001));

    nothing_document_wide(
        &counted,
        "one of several identical paragraphs changing brush",
    );
    assert_eq!(
        counted.paragraphs_evicted, 0,
        "the old key is still active in three sibling contexts and must remain cached"
    );
    assert_eq!(
        counted.text_shaped, 1,
        "only the one context with the new brush needs a new shaped result"
    );
    assert_eq!(
        every_grey(&mut harness),
        [120, 120, 10, 10, 10, 10, 10, 10, 10, 10],
        "the changed context and the siblings sharing its old key were not independently drawn"
    );
}

#[test]
fn every_element_leaving_the_slot_at_once_reshapes_every_one_of_them() {
    let _turn = zgui_profile::counter::exclusive();
    let (_dark, hot, mut harness) = opened();

    // The complementary case, and the one that says the invalidation was scoped rather than
    // removed: four elements leave the slot together, and four paragraphs have to go.
    let counted = moved(&mut harness, |_| hot.set(0b1111));

    nothing_document_wide(&counted, "every element leaving a shared slot");
    assert_eq!(
        counted.paragraphs_evicted, 4,
        "four elements left the slot they were sharing and {} paragraphs were thrown away",
        counted.paragraphs_evicted
    );
    assert!(
        counted.text_shaped >= 4,
        "four elements had their shaping thrown away and only {} runs were shaped again",
        counted.text_shaped
    );
    assert_eq!(
        every_grey(&mut harness),
        [120, 120, 120, 120, 120, 120, 120, 120, 10, 10],
        "the four elements that left the slot are not all drawn in their new colour"
    );
}

#[test]
fn a_theme_change_rewrites_every_brush_and_reshapes_nothing() {
    let _turn = zgui_profile::counter::exclusive();
    let (dark, _hot, mut harness) = opened();

    // The one change that moves *every* colour at once is the one the brush table exists for: every
    // element drawn through a slot moves in the same batch, so the slot is rewritten where it
    // stands and not one glyph is shaped again.
    let counted = moved(&mut harness, |_| dark.set(true));

    nothing_document_wide(&counted, "a theme change");
    assert_eq!(
        counted.paragraphs_evicted, 0,
        "a theme change re-shaped text, which is the cost the brush table exists to avoid"
    );
    assert_eq!(
        every_grey(&mut harness),
        [210; 10],
        "the theme changed and some of the window's text is still drawn in the old colour"
    );
}
