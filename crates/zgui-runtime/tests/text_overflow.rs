//! Where a clipped line is cut, asked of a window rather than of a measurer.
//!
//! `text-overflow` is decided in layout, and it is the one layout decision that reads a paragraph's
//! **clusters** rather than its line boxes: the mark goes on a character boundary, so layout asks
//! its measurer where the boundaries are. Every test in the layout crate asks a measurer directly.
//! A window asks one through [`TextEngine`], behind a box, and that indirection is the whole of the
//! difference between the two — which is how a wrapper that forwarded every sizing question and
//! answered "no clusters" to that one stayed green everywhere while every clipped label in every
//! real application drew its mark and none of its words.
//!
//! So the assertions here are about a *window*: where the cut landed, and what the frame drew
//! either side of it. The fixed face makes both computable by hand — one cluster is half the font
//! size, eight pixels at the initial size — so every expected boundary is arithmetic rather than a
//! recording.
//!
//! [`TextEngine`]: zgui_runtime::TextEngine

mod support;

use std::time::Duration;

use zgui_geom::{Css, CssPx, DevicePx, Point, Size};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

/// One cluster's advance at the initial font size, in device pixels.
const ADVANCE: f32 = 8.0;

/// The words in the clipped box, which need sixteen clusters.
const WORDS: &str = "abcdefghijklmnop";

/// The block the paragraph is in. The padding keeps the line box away from the surface corner, so
/// an assertion about a coordinate cannot pass for a frame that ignored the placement.
const ROOT: &str = "root { display: block; width: 400px; height: 300px; padding: 12px 20px }";

/// The clipped box: twelve clusters wide, one line, marked at its end.
const ELLIPSIS: &str = "text { display: block; width: 96px; overflow: hidden;
                               text-overflow: ellipsis; white-space: nowrap }";

/// How many of the words survive that: twelve fit, and the mark occupies the last one.
const KEPT: usize = 11;

/// A window holding one paragraph too wide for its box.
fn clipped(css: &str) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(css, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().child(WORDS))
                .into_view()
                .build(cx),
        )
    })
}

/// The cut the first inline formatting context in the window recorded.
fn cut(window: &zgui_runtime::Window) -> zgui_layout::inline::ellipsis::LineEllipsis {
    let layout = window.layout().borrow();
    for key in layout.keys() {
        let Some(resolution) = layout.inline_resolution(key) else {
            continue;
        };
        if let Some(cut) = resolution.lines.first().and_then(|line| line.ellipsis) {
            return cut;
        }
    }
    panic!("no line in the window was cut off at all");
}

/// A line too wide for its box is cut on the boundary its clusters put there.
#[test]
fn a_window_cuts_a_clipped_line_where_its_clusters_say() {
    let mut app = clipped(&format!("{ROOT} {ELLIPSIS}"));
    app.settle(8);

    let window = &app.app().windows()[0];
    let cut = cut(window);

    assert!(!cut.at_start, "the line overflows its end, not its start");
    assert_eq!(
        cut.cutoff,
        KEPT as f32 * ADVANCE,
        "the cut must fall on the last cluster boundary that leaves room for the mark. Zero here \
         means the measurer reported no clusters at all, which cuts every clipped line to nothing"
    );
}

/// The words that fit are drawn, and the mark stands after the last of them.
///
/// The claim the layout assertion cannot make. A cut recorded in the box tree is not a cut on the
/// screen, and what a reader sees is a line of words with a mark at the end of it — so this counts
/// the glyphs that survived the cut as well as placing the mark. A cut at zero clips the line's own
/// glyphs away entirely and leaves this frame holding the mark and nothing else.
#[test]
fn the_words_that_fit_are_drawn_and_the_mark_stands_after_them() {
    let mut app = clipped(&format!("{ROOT} {ELLIPSIS}"));
    app.settle(8);

    let window = &app.app().windows()[0];
    let line = support::first_line_box(window);
    let sprites = &window.scene().primitives.mono_sprites;

    assert_eq!(
        sprites.len(),
        KEPT + 1,
        "eleven letters fit and the mark is one more sprite: {sprites:?}"
    );
    for (index, sprite) in sprites.iter().take(KEPT).enumerate() {
        assert_eq!(
            sprite.ink().origin.x,
            DevicePx(line.origin.x.0 + index as f32 * ADVANCE),
            "letter {index} is not at the pen position its own advances put it at"
        );
    }

    let mark = sprites
        .last()
        .expect("the mark is drawn after the line it marks");
    assert_eq!(
        mark.ink().origin.x,
        DevicePx(line.origin.x.0 + KEPT as f32 * ADVANCE),
        "the mark stands where the content was cut"
    );
    assert_ne!(
        mark.clip, sprites[0].clip,
        "the mark is drawn through the line's untightened clip and the line's own glyphs through \
         the tightened one, so a mark sharing their clip is a twelfth letter rather than a mark"
    );
}

/// The string form reserves the room its own string needs, and cuts the line that much earlier.
#[test]
fn a_string_mark_is_measured_as_the_string_it_names() {
    let sheet = format!(
        "{ROOT} text {{ display: block; width: 96px; overflow: hidden;
                        text-overflow: \"...\"; white-space: nowrap }}"
    );
    let mut app = clipped(&sheet);
    app.settle(8);

    let window = &app.app().windows()[0];
    assert_eq!(
        cut(window).cutoff,
        9.0 * ADVANCE,
        "three characters of mark leave room for nine of the twelve"
    );
    assert_eq!(
        window.scene().primitives.mono_sprites.len(),
        9 + 3,
        "nine letters and a three-character mark"
    );
}

/// A scrolled list of cut lines keeps its words as well as its marks.
///
/// The regression this stands against: a cut line's window is minted where the line is drawn on
/// the frame that encodes it, and a scrolled row replays its chunk with an offset. A window left
/// at the encode position cuts every replayed row against where it *was* — the words shrink away
/// as the scroll runs, past one row height only the marks are left, and hovering a row restores
/// it by re-encoding. So: scroll by whole rows, force a full repaint out of nothing but replays,
/// and count what a congruent picture has to hold.
#[test]
fn scrolled_rows_keep_their_words_and_their_marks() {
    let sheet = format!(
        "{ROOT}
         .port {{ display: block; width: 200px; height: 120px; overflow: scroll }}
         .row {{ display: block; width: 96px; height: 20px; line-height: 20px;
                 overflow: hidden; text-overflow: ellipsis; white-space: nowrap }}"
    );
    let mut app = support::app_with_text(&sheet, |cx: &mut BuildCx<'_>| {
        let mut port = zgui_elements::column().class("port");
        for _ in 0..12 {
            port = port.child(zgui_elements::text().class("row").child(WORDS));
        }
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(port)
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);

    // A full repaint before and after, so both counts describe the whole port rather than the
    // last frame's damage band. Un-occluding damages everything and moves nothing, which is what
    // makes the second repaint entirely a matter of replays.
    let full_repaint = |app: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>| {
        app.deliver_to_first(SurfaceEvent::Occluded(true));
        app.deliver_to_first(SurfaceEvent::Occluded(false));
        app.settle(8);
    };
    full_repaint(&mut app);
    let rows = 120 / 20;
    let before = app.app().windows()[0].scene().primitives.mono_sprites.len();
    assert_eq!(
        before,
        rows * (KEPT + 1),
        "the control: six rows of eleven letters and a mark each"
    );

    // Two whole rows, so the settled picture is congruent with the one before it.
    app.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(100.0), CssPx(60.0)),
            delta: ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(40.0))),
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    for _ in 0..30 {
        app.advance(Duration::from_millis(20));
        app.pump();
    }
    app.settle(8);

    full_repaint(&mut app);
    let after = app.app().windows()[0].scene().primitives.mono_sprites.len();
    assert_eq!(
        after, before,
        "a scrolled row lost its words: every replayed line must draw what it drew, moved"
    );
}

/// A line pushed out of the start of its box is cut at that end, on a boundary of its own.
///
/// The other end of the same walk, and the shape of the failure that leaves *nothing* readable: a
/// start-side cut that finds no clusters falls back to the line's far edge, which hides every word
/// on the line behind the mark.
#[test]
fn a_line_that_starts_before_its_box_is_cut_at_its_start() {
    let sheet = format!(
        "{ROOT} text {{ display: block; width: 96px; overflow: hidden; text-indent: -40px;
                        text-overflow: ellipsis ellipsis; white-space: nowrap }}"
    );
    let mut app = clipped(&sheet);
    app.settle(8);

    let window = &app.app().windows()[0];
    let cut = cut(window);

    assert!(cut.at_start, "the line begins forty pixels before its box");
    assert_eq!(
        cut.cutoff,
        -40.0 + 6.0 * ADVANCE,
        "the first cluster boundary at or past the far edge of the mark, measured from the \
         content box's start edge"
    );
    assert!(
        window.scene().primitives.mono_sprites.len() > 1,
        "the words after the cut are still drawn; one sprite is the mark standing alone"
    );
}
