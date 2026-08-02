//! The insertion point and the selection reaching the screen, the pointer, and the clipboard.
//!
//! Everything here runs through a real window on the headless platform: real platform events, the
//! real dispatch, the real layout, the real emit walk, and the display list the renderer was handed.
//! That is the point. The editing model's own crate proves that the model knows where the caret is;
//! nothing there can prove that anything ever *draws* it, and a caret that is computed and never
//! painted passes every model test there is while a person sees a field they cannot find their
//! place in.
//!
//! The fixed face makes the numbers computable rather than recorded: one cluster is eight device
//! pixels wide at the initial size, so an offset and an x are the same fact twice and a test can say
//! which one it means.

mod support;

use std::time::Duration;

use zgui_geom::{CssPx, Device, DevicePx, Point, Rect};
use zgui_platform::{ClipboardFormat, ClipboardKind, PlatformCx, SurfaceEvent};
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{
    Key, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction,
    PointerEvent, Timestamp,
};

/// The sheet the fixture is styled by.
///
/// The padding is load-bearing: with the field at the window's corner every assertion below would
/// hold for an emitter that ignored where the line box landed and drew the caret at the origin.
/// The colour is load-bearing too, and deliberately not a colour anything defaults to: a caret
/// hard-coded black would be indistinguishable from one that read `currentColor` if the field were
/// left at the initial colour, and a black caret on a dark theme is an invisible one.
const CSS: &str = "root { display: block; width: 400px; height: 300px; padding: 12px 20px }
                   editor { display: block; width: 200px; height: 40px; color: rgb(0, 102, 204) }
                   text { display: block; color: rgb(0, 102, 204) }";

/// One cluster's advance in device pixels, from the fixed face at the initial font size.
const ADVANCE: f32 = 8.0;

/// What the stylesheet writes the field's text in, which is what the caret has to be drawn in.
const INK: [f32; 3] = [0.0, 102.0 / 255.0, 204.0 / 255.0];

/// How opaque a selection band is over the text it marks.
const BAND_ALPHA: f32 = 0.30;

/// One scripted window holding an editor.
struct Script {
    /// The window being driven.
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// The editable element.
    editor: NodeRef,
}

impl Script {
    /// Delivers one surface event and lets the frames it produced settle.
    fn deliver(&mut self, event: SurfaceEvent) {
        self.harness.deliver_to_first(event);
        self.harness.settle(8);
    }

    /// Presses a named key.
    fn press_named(&mut self, key: NamedKey, code: KeyCode) {
        self.press(
            KeyEvent::named(key, PhysicalKey::Code(code)),
            Modifiers::NONE,
        );
    }

    /// Presses one key.
    fn press(&mut self, event: KeyEvent, modifiers: Modifiers) {
        self.deliver(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event,
            modifiers,
            timestamp: Timestamp::ORIGIN,
        });
    }

    /// Presses one letter with the command modifier held, which is what a shortcut is.
    fn shortcut(&mut self, letter: &str) {
        let event = KeyEvent {
            key: Key::Character(letter.into()),
            key_without_modifiers: Key::Character(letter.into()),
            physical: PhysicalKey::Code(KeyCode::KeyA),
            location: zgui_vocab::KeyLocation::Standard,
            repeat: false,
        };
        self.press(event, Modifiers::CONTROL);
    }

    /// Delivers one pointer action at a point on the surface, in CSS pixels.
    fn point(&mut self, action: PointerAction, x: f32, y: f32) {
        self.deliver(SurfaceEvent::Pointer {
            action,
            event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(y))),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }

    /// What the framework reports as selected in the editor.
    fn selection(&self) -> Option<core::ops::Range<usize>> {
        self.editor.selection()
    }

    /// The window.
    fn window(&self) -> &zgui_runtime::Window {
        &self.harness.app().windows()[0]
    }

    /// The first line fragment's box, which is what every rectangle here is measured against.
    fn line(&self) -> Rect<DevicePx, Device> {
        support::first_line_box(self.window())
    }

    /// Every quad in the display list, with the colour it is filled in and where it sorts.
    ///
    /// Quads and not "primitives": the root's background is a quad too, and a count would be
    /// satisfied by it. Every assertion below names the rectangle it wants. The fill is resolved
    /// through the scene's own paint table rather than assumed, because the reference a quad
    /// carries is an index and an index proves nothing about a colour.
    fn quads(&self) -> Vec<Painted> {
        let window = self.window();
        let scene = window.scene();
        scene
            .primitives
            .quads
            .iter()
            .map(|quad| Painted {
                rect: Rect::new(
                    Point::new(DevicePx(quad.bounds[0]), DevicePx(quad.bounds[1])),
                    zgui_geom::Size::new(DevicePx(quad.bounds[2]), DevicePx(quad.bounds[3])),
                ),
                fill: quad
                    .fill
                    .id()
                    .and_then(|id| scene.paints.get(id))
                    .and_then(|paint| match paint {
                        zgui_scene::Paint::Solid(color) => Some(*color),
                        _ => None,
                    }),
                order: quad.order,
            })
            .collect()
    }

    /// How many glyph sprites the display list holds.
    ///
    /// Read beside every caret assertion, so that "the caret is not there" can be told from "this
    /// frame drew nothing at all".
    fn glyphs(&self) -> usize {
        self.window().scene().primitives.mono_sprites.len()
    }

    /// Every glyph sprite of the text, as a rectangle and the order it sorts at.
    ///
    /// Draw order in this scene is allocated against overlap — two primitives that cannot overlap
    /// are free to share an order, and here the glyphs left of a band and the glyphs under it
    /// genuinely take different ones. So a depth claim has to name the glyphs it is a claim about:
    /// "above every glyph in the document" is false of a correct caret, and asserting it would send
    /// somebody to fix a painter that is right.
    fn glyph_sprites(&self) -> Vec<(Rect<DevicePx, Device>, u32)> {
        self.window()
            .scene()
            .primitives
            .mono_sprites
            .iter()
            .map(|sprite| {
                (
                    Rect::new(
                        Point::new(DevicePx(sprite.bounds[0]), DevicePx(sprite.bounds[1])),
                        zgui_geom::Size::new(
                            DevicePx(sprite.bounds[2]),
                            DevicePx(sprite.bounds[3]),
                        ),
                    ),
                    sprite.order,
                )
            })
            .collect()
    }

    /// The glyphs a rectangle lands on top of, and the order each of them sorts at.
    fn glyphs_under(&self, rect: Rect<DevicePx, Device>) -> Vec<u32> {
        self.glyph_sprites()
            .into_iter()
            .filter(|(bounds, _)| overlaps(*bounds, rect))
            .map(|(_, order)| order)
            .collect()
    }

    /// Whether a caret — one device pixel wide, line-high, in the field's own ink, in front of the
    /// glyphs — stands at `x` on the first line.
    ///
    /// Every clause earns its place. Without the width a selection band answers; without the height
    /// a stray rule does; without the colour a caret painted in whatever the emitter had to hand
    /// answers, and a caret the same colour as the background it sits on is one a person cannot
    /// see; without the order it is painted underneath the letter it is sitting on, which is the
    /// same as not painting it.
    fn caret_at(&self, x: f32) -> bool {
        let line = self.line();
        let want = Rect::new(
            Point::new(DevicePx(x), line.origin.y),
            zgui_geom::Size::new(DevicePx(1.0), line.size.height),
        );
        let under = self.glyphs_under(want);
        self.quads().iter().any(|quad| {
            quad.covers(want)
                && quad.filled_with(INK, 1.0)
                && under.iter().all(|&order| quad.order > order)
        })
    }

    /// Whether a selection band covering `rect` is drawn behind the glyphs in the field's ink.
    ///
    /// The bands the framework draws always have glyphs over them — that is what a selection is —
    /// so this refuses a rectangle with nothing under it rather than passing on the strength of an
    /// empty `all`, which is how a depth claim goes vacuous.
    fn band_over(&self, rect: Rect<DevicePx, Device>) -> bool {
        let under = self.glyphs_under(rect);
        !under.is_empty()
            && self.quads().iter().any(|quad| {
                quad.covers(rect)
                    && quad.filled_with(INK, BAND_ALPHA)
                    && under.iter().all(|&order| quad.order < order)
            })
    }

    /// Forces the whole surface to be redrawn, so the display list holds everything again.
    ///
    /// A frame emits only what its damage reaches, so a caret that is still on the screen is absent
    /// from the display list of any frame that damaged nothing near it. Reading such a frame and
    /// concluding the caret has gone is the mistake this exists to prevent: an un-occlusion is what
    /// a real compositor produces after a window has been hidden, and it repaints the lot.
    fn repaint_all(&mut self) {
        self.deliver(SurfaceEvent::Occluded(true));
        self.deliver(SurfaceEvent::Occluded(false));
        assert_eq!(
            self.glyphs(),
            3,
            "the forced repaint did not redraw the text, so nothing below it is a reading"
        );
    }

    /// Every quad one device pixel wide, which is the shape only a caret has here.
    ///
    /// Deliberately looser than [`Script::caret_at`]: this is what a failure message prints and
    /// what "no caret anywhere" is asserted over, so it must catch a caret drawn in the wrong
    /// colour or at the wrong depth rather than filtering it out and reporting nothing.
    fn carets(&self) -> Vec<Painted> {
        self.quads()
            .into_iter()
            .filter(|quad| (quad.rect.size.width.0 - 1.0).abs() < 0.01)
            .collect()
    }
}

/// Whether two rectangles share any area at all.
fn overlaps(a: Rect<DevicePx, Device>, b: Rect<DevicePx, Device>) -> bool {
    a.origin.x.0 < b.origin.x.0 + b.size.width.0
        && b.origin.x.0 < a.origin.x.0 + a.size.width.0
        && a.origin.y.0 < b.origin.y.0 + b.size.height.0
        && b.origin.y.0 < a.origin.y.0 + a.size.height.0
}

/// One quad out of the display list: where it lands, what fills it, and where it sorts.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Painted {
    /// Where it lands, in device pixels.
    rect: Rect<DevicePx, Device>,
    /// The flat colour filling it, and nothing when it is filled with something else.
    fill: Option<zgui_color::Color>,
    /// Where it sorts against everything else in the display list.
    order: u32,
}

impl Painted {
    /// Whether this lands exactly on `rect`.
    fn covers(&self, rect: Rect<DevicePx, Device>) -> bool {
        (self.rect.origin.x.0 - rect.origin.x.0).abs() < 0.01
            && (self.rect.origin.y.0 - rect.origin.y.0).abs() < 0.01
            && (self.rect.size.width.0 - rect.size.width.0).abs() < 0.01
            && (self.rect.size.height.0 - rect.size.height.0).abs() < 0.01
    }

    /// Whether it is filled with `components` at `alpha`.
    fn filled_with(&self, components: [f32; 3], alpha: f32) -> bool {
        let Some(color) = self.fill else {
            return false;
        };
        (color.alpha() - alpha).abs() < 0.005
            && color
                .components()
                .iter()
                .zip(components)
                .all(|(got, want)| (got - want).abs() < 0.01)
    }
}

/// A window holding an editor with `content` in it, focused, with its caret at the start.
fn scripted(content: &'static str) -> Script {
    let editor = NodeRef::new();
    let harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::editor().node_ref(editor).child(content))
                .into_view()
                .build(cx),
        )
    });
    let mut script = Script { harness, editor };
    script.harness.settle(8);
    script.press_named(NamedKey::Tab, KeyCode::Tab);
    assert_eq!(
        script.selection(),
        Some(0..0),
        "focusing an editable element has to give it a caret, or there is nothing to paint"
    );
    script
}

#[test]
fn a_focused_field_paints_a_caret_at_the_offset_the_model_reports() {
    let mut script = scripted("abc");
    let line = script.line();
    assert!(
        line.origin.x.0 > 0.0 && line.origin.y.0 > 0.0,
        "the fixture must not put the line at the surface corner, or nothing below is a test"
    );
    assert_eq!(script.glyphs(), 3, "the fixture's own text was never drawn");
    assert!(
        script.caret_at(line.origin.x.0),
        "a focused field drew no caret at offset zero: {:?}",
        script.quads()
    );

    // Walk the caret twice and follow it. Where it must be is derived from the offset the model
    // reports, times the advance of the face that shaped the text — not from where it was found.
    for _ in 0..2 {
        script.press_named(NamedKey::ArrowRight, KeyCode::ArrowRight);
    }
    let offset = script.selection().expect("the model reports a caret").start;
    assert_eq!(offset, 2);
    let want = Rect::new(
        Point::new(
            DevicePx(line.origin.x.0 + offset as f32 * ADVANCE),
            line.origin.y,
        ),
        zgui_geom::Size::new(DevicePx(1.0), line.size.height),
    );
    // The depth clause inside `caret_at` is a claim about the glyphs the caret lands on, and an
    // empty set of those would satisfy it however the painter sorted. Here the caret stands on the
    // third letter, so the set is not empty and the clause is a claim.
    assert!(
        !script.glyphs_under(want).is_empty(),
        "the caret was placed where no glyph is, so its depth is not being tested here"
    );
    assert!(
        script.caret_at(want.origin.x.0),
        "the caret is not at the offset the model reports, in the field's ink, over the letter it \
         stands on: {:?} against a line at {line:?} and glyphs {:?}",
        script.carets(),
        script.glyph_sprites()
    );
    assert!(
        !script.caret_at(line.origin.x.0),
        "and it is no longer at the offset it left"
    );
}

#[test]
fn the_caret_blinks_over_the_virtual_clock_and_settles_visible() {
    use zgui_runtime::caret::blink::{HALF_PERIOD, PHASES};

    let mut script = scripted("abc");
    let line = script.line();
    assert!(script.caret_at(line.origin.x.0), "the caret starts shown");

    // The dark phase. The glyph count is read with it: a frame that drew nothing at all would
    // satisfy "no caret" while proving nothing.
    script.harness.advance(HALF_PERIOD);
    script.harness.settle(8);
    assert_eq!(script.glyphs(), 3, "the dark frame drew no text either");
    assert!(
        !script.caret_at(line.origin.x.0),
        "the caret never went dark, so it does not blink: {:?}",
        script.carets()
    );

    script.harness.advance(HALF_PERIOD);
    script.harness.settle(8);
    assert_eq!(
        script.glyphs(),
        3,
        "the frame that brought it back drew no text"
    );
    assert!(
        script.caret_at(line.origin.x.0),
        "the caret went dark and never came back: {:?}",
        script.quads()
    );

    // It settles *on*. A caret that settled dark would be a field with no insertion point in it
    // for as long as nobody typed, which is exactly the state a person leaves a field in. The
    // surface is repainted whole first, because a settled caret damages nothing and a frame that
    // damaged nothing emits nothing — which would read as an absent caret whatever the truth is.
    script
        .harness
        .advance(HALF_PERIOD * PHASES + Duration::from_millis(1));
    script.harness.settle(8);
    script.repaint_all();
    assert!(
        script.caret_at(line.origin.x.0),
        "the blink settled dark rather than visible: {:?}",
        script.quads()
    );
    script.harness.advance(HALF_PERIOD * 4);
    script.harness.settle(8);
    script.repaint_all();
    assert!(
        script.caret_at(line.origin.x.0),
        "a settled caret started blinking again"
    );
}

#[test]
fn a_click_places_the_caret_at_the_offset_that_coordinate_holds() {
    let mut script = scripted("abcdef");
    let line = script.line();
    // Two and a half advances in: past the middle of the third cluster, so the caret belongs after
    // it. The offset is computed from the face's advance rather than read back from the framework.
    let x = line.origin.x.0 + ADVANCE * 2.5;
    let y = line.origin.y.0 + line.size.height.0 / 2.0;
    script.point(PointerAction::Pressed, x, y);

    assert_eq!(
        script.selection(),
        Some(3..3),
        "a click in the second half of the third cluster belongs after it"
    );
    assert!(
        script.caret_at(line.origin.x.0 + ADVANCE * 3.0),
        "the caret was placed in the model and not painted: {:?}",
        script.carets()
    );

    // And a click on the leading half of a cluster lands before it, which is the other side of the
    // same round trip: without it, every click could be answered by rounding down.
    let x = line.origin.x.0 + ADVANCE * 1.2;
    script.point(PointerAction::Pressed, x, y);
    assert_eq!(script.selection(), Some(1..1));
    assert!(script.caret_at(line.origin.x.0 + ADVANCE));
}

#[test]
fn a_drag_selects_the_range_between_its_two_points_and_paints_a_band_over_it() {
    let mut script = scripted("abcdef");
    let line = script.line();
    let y = line.origin.y.0 + line.size.height.0 / 2.0;

    script.point(PointerAction::Pressed, line.origin.x.0 + ADVANCE * 0.2, y);
    assert_eq!(script.selection(), Some(0..0), "the press is a caret");
    script.point(PointerAction::Moved, line.origin.x.0 + ADVANCE * 3.2, y);

    assert_eq!(
        script.selection(),
        Some(0..3),
        "the drag selected the wrong range"
    );
    let expected = Rect::new(
        line.origin,
        zgui_geom::Size::new(DevicePx(ADVANCE * 3.0), line.size.height),
    );
    assert!(
        script.band_over(expected),
        "no band covering exactly the selected clusters, in the field's ink, behind its glyphs: \
         wanted {expected:?} at {INK:?} × {BAND_ALPHA}, got {:?}",
        script.quads()
    );
    assert!(
        script.caret_at(line.origin.x.0 + ADVANCE * 3.0),
        "the caret has to sit at the moving end of the drag"
    );

    // Releasing ends the drag: a pointer that merely crosses the field afterwards selects nothing.
    script.point(PointerAction::Released, line.origin.x.0 + ADVANCE * 3.2, y);
    script.point(PointerAction::Moved, line.origin.x.0 + ADVANCE * 5.2, y);
    assert_eq!(
        script.selection(),
        Some(0..3),
        "a move after the release went on extending the selection"
    );
}

#[test]
fn a_drag_backwards_selects_the_same_range_and_puts_the_caret_at_the_moving_end() {
    let mut script = scripted("abcdef");
    let line = script.line();
    let y = line.origin.y.0 + line.size.height.0 / 2.0;

    script.point(PointerAction::Pressed, line.origin.x.0 + ADVANCE * 4.2, y);
    script.point(PointerAction::Moved, line.origin.x.0 + ADVANCE * 1.2, y);
    assert_eq!(script.selection(), Some(1..4));
    assert!(
        script.caret_at(line.origin.x.0 + ADVANCE),
        "a backwards drag draws its caret at the end that moved, which is the lower one: {:?}",
        script.carets()
    );
}

#[test]
fn a_copy_puts_the_selected_text_on_the_platform_clipboard() {
    let mut script = scripted("abcdef");
    script.shortcut("a");
    assert_eq!(
        script.selection(),
        Some(0..6),
        "select-all selected nothing"
    );
    script.shortcut("c");

    let held = script
        .harness
        .platform()
        .clipboard()
        .read_blocking(ClipboardKind::Standard, ClipboardFormat::Text)
        .expect("a copy has to reach the clipboard");
    assert_eq!(
        held,
        zgui_platform::ClipboardData::Text("abcdef".into()),
        "the copy reached the model and stopped there"
    );
}

#[test]
fn a_cut_puts_the_text_on_the_clipboard_and_takes_it_out_of_the_document() {
    let mut script = scripted("abcdef");
    script.shortcut("a");
    script.shortcut("x");

    let held = script
        .harness
        .platform()
        .clipboard()
        .read_blocking(ClipboardKind::Standard, ClipboardFormat::Text)
        .expect("a cut has to reach the clipboard");
    assert_eq!(held, zgui_platform::ClipboardData::Text("abcdef".into()));
    assert_eq!(
        script.glyphs(),
        0,
        "the cut text is still on the screen, so it was copied and not cut"
    );
    assert_eq!(script.selection(), Some(0..0));
}

#[test]
fn an_element_nobody_can_type_into_gets_no_caret_from_a_click() {
    // A caret painted into a paragraph is a framework that lets a person type into text they
    // cannot edit. Proving that requires the *same* window to be able to produce a caret at all:
    // "no caret appeared" is satisfied by a click that missed, by a fixture with no editable
    // element in it, and by a caret feature that was never wired up — so the paragraph and the
    // field stand side by side here and the identical click is delivered to each in turn.
    let plain = NodeRef::new();
    let field = NodeRef::new();
    let harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().node_ref(plain).child("abcdef"))
                .child(zgui_elements::editor().node_ref(field).child("abcdef"))
                .into_view()
                .build(cx),
        )
    });
    let mut script = Script {
        harness,
        editor: plain,
    };
    script.harness.settle(8);
    // The paragraph is the column's first child and the field its second, so the paragraph's line
    // is the higher of the two. Sorted rather than indexed: the layout reports its line boxes in
    // the order its tree walk produced, which is not document order, and a test that indexed into
    // that list would quietly click the wrong element while reading as if it clicked the right one.
    let mut lines = support::line_boxes(script.window());
    lines.sort_by(|a, b| a.origin.y.0.total_cmp(&b.origin.y.0));
    assert_eq!(
        lines.len(),
        2,
        "the fixture must hold both a paragraph and a field"
    );
    let (paragraph, editable) = (lines[0], lines[1]);
    assert!(
        (paragraph.origin.x.0 - editable.origin.x.0).abs() < 0.01,
        "the two lines have to start at the same x, or the two clicks are not the same click"
    );
    assert!(
        (paragraph.origin.y.0 - editable.origin.y.0).abs() > 1.0,
        "the two lines have to be at different heights, or one click lands on both"
    );

    let inset = ADVANCE * 2.5;
    script.point(
        PointerAction::Pressed,
        paragraph.origin.x.0 + inset,
        paragraph.origin.y.0 + paragraph.size.height.0 / 2.0,
    );
    assert_eq!(script.selection(), None, "a paragraph reported a selection");
    assert!(
        script.carets().is_empty(),
        "something drew a caret into an element nobody can type into: {:?}",
        script.carets()
    );

    // The control. Without it the assertion above is satisfied by a framework that never draws a
    // caret anywhere, which is exactly the defect this file exists to catch.
    script.editor = field;
    script.point(
        PointerAction::Pressed,
        editable.origin.x.0 + inset,
        editable.origin.y.0 + editable.size.height.0 / 2.0,
    );
    assert_eq!(
        script.selection(),
        Some(3..3),
        "the identical click on the field placed no caret, so the negative above proves nothing"
    );
    let caret = Rect::new(
        Point::new(
            DevicePx(editable.origin.x.0 + ADVANCE * 3.0),
            editable.origin.y,
        ),
        zgui_geom::Size::new(DevicePx(1.0), editable.size.height),
    );
    assert!(
        script
            .quads()
            .iter()
            .any(|quad| quad.covers(caret) && quad.filled_with(INK, 1.0)),
        "the control click drew no caret either: {:?}",
        script.carets()
    );
    // And it drew exactly one: a caret in the paragraph as well would still leave the field's own
    // caret in the list and satisfy every assertion above.
    assert_eq!(
        script.carets().len(),
        1,
        "more than one caret is on the screen: {:?}",
        script.carets()
    );
}

#[test]
fn a_caret_on_the_second_line_of_a_wrapped_field_is_drawn_on_that_line() {
    // Everything else in this file is a single line, where "the line box" and "the paragraph" are
    // the same rectangle and a painter that ignored which line a caret is on would still be right.
    // A wrapped field separates them: the caret has to move down by a line box and back to the
    // left margin, and its own line is the one that has to carry it.
    let editor = NodeRef::new();
    let css = "root { display: block; width: 400px; height: 300px; padding: 12px 20px }
               editor { display: block; width: 48px; height: 120px; color: rgb(0, 102, 204) }";
    let harness = support::app_with_text(css, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::editor().node_ref(editor).child("abc def"))
                .into_view()
                .build(cx),
        )
    });
    let mut script = Script { harness, editor };
    script.harness.settle(8);
    script.press_named(NamedKey::Tab, KeyCode::Tab);

    let mut lines = support::line_boxes(script.window());
    lines.sort_by(|a, b| a.origin.y.0.total_cmp(&b.origin.y.0));
    assert_eq!(
        lines.len(),
        2,
        "the field did not wrap, so this proves nothing"
    );
    let (first, second) = (lines[0], lines[1]);
    assert!(
        second.origin.y.0 > first.origin.y.0,
        "the second line has to be below the first"
    );

    // Where the second line starts in the text is the shaper's business — a break may leave the
    // space that caused it at the end of one line or the start of the next — so it is asked for
    // rather than assumed: a click on the line's left edge is by definition its first offset.
    let y = second.origin.y.0 + second.size.height.0 / 2.0;
    script.point(PointerAction::Pressed, second.origin.x.0 + 0.1, y);
    let base = script.selection().expect("the click placed a caret").start;
    assert!(
        base > 0,
        "the click landed at the start of the text, so it did not reach the second line"
    );

    let want = Rect::new(
        second.origin,
        zgui_geom::Size::new(DevicePx(1.0), second.size.height),
    );
    assert!(
        script
            .quads()
            .iter()
            .any(|quad| quad.covers(want) && quad.filled_with(INK, 1.0)),
        "the caret for offset {base} is not at the second line's left edge {want:?}: {:?}",
        script.carets()
    );
    // Exactly one: a painter that put a caret on every line would satisfy the assertion above
    // while showing an insertion point in two places at once.
    assert_eq!(
        script.carets().len(),
        1,
        "more than one caret is on the screen: {:?}",
        script.carets()
    );

    // And it advances along its own line rather than along the paragraph. Without this the caret
    // could be drawn from the paragraph origin plus the whole-text offset and still land correctly
    // on the left edge, which is the one x where the two readings agree.
    script.press_named(NamedKey::ArrowRight, KeyCode::ArrowRight);
    assert_eq!(script.selection(), Some(base + 1..base + 1));
    let want = Rect::new(
        Point::new(DevicePx(second.origin.x.0 + ADVANCE), second.origin.y),
        zgui_geom::Size::new(DevicePx(1.0), second.size.height),
    );
    assert!(
        script
            .quads()
            .iter()
            .any(|quad| quad.covers(want) && quad.filled_with(INK, 1.0)),
        "the caret did not advance one cluster along the second line to {want:?}: {:?}",
        script.carets()
    );
    assert_eq!(script.carets().len(), 1);
}

/// A field a person has emptied still shows where the next character will go.
///
/// The one paragraph in the framework that holds nothing at all. A field that has never been typed
/// into usually shows a placeholder, which is generated content and therefore text — so the shape
/// this covers is the field that *started* with something and had all of it deleted, which is the
/// state a person leaves a field in whenever they mean to replace what is in it.
///
/// Three things have to be true at once and each fails on its own: the emptied paragraph still has
/// a line box, the model's offset still resolves to a place on it, and the frame still paints
/// something there.
#[test]
fn a_field_emptied_of_everything_still_shows_its_caret() {
    use zgui_runtime::caret::blink::HALF_PERIOD;

    let mut script = scripted("abc");
    let line = script.line();
    assert!(
        script.caret_at(line.origin.x.0),
        "the field shows no caret even with text in it, so nothing below is a test"
    );

    for _ in 0..3 {
        script.press_named(NamedKey::Delete, KeyCode::Delete);
    }
    assert_eq!(script.selection(), Some(0..0), "the field was not emptied");
    assert_eq!(
        script.glyphs(),
        0,
        "the field still holds text, so this is not the state being tested"
    );

    // The line box outlives its text. Without it there is nothing for a caret to be drawn on and
    // nothing to draw it, whatever the model says.
    let empty = script.line();
    assert_eq!(
        (empty.origin.x.0, empty.origin.y.0),
        (line.origin.x.0, line.origin.y.0),
        "the emptied field's line box is not where its text was"
    );
    assert!(
        empty.size.height.0 > 0.0,
        "the emptied field's line box collapsed to nothing"
    );

    // Read over a blink, because a frame that damaged nothing draws nothing and a caret that is
    // still on the screen is absent from such a frame's display list. Both phases are asserted:
    // "always drawn" and "correctly drawn" are the same picture on the lit frame alone.
    script.harness.advance(HALF_PERIOD);
    script.harness.settle(8);
    assert!(
        !script.caret_at(empty.origin.x.0),
        "the emptied field's caret never goes dark, so it is not the caret: {:?}",
        script.carets()
    );
    script.harness.advance(HALF_PERIOD);
    script.harness.settle(8);
    assert!(
        script.caret_at(empty.origin.x.0),
        "the emptied field draws no caret at the start of its line: {:?}",
        script.carets()
    );
    assert_eq!(
        script.carets().len(),
        1,
        "more than one caret is on the screen: {:?}",
        script.carets()
    );

    // And it is a caret rather than a rectangle that happens to be there: typing puts it back
    // where the first character ends.
    script.press(KeyEvent::character("x"), Modifiers::NONE);
    assert_eq!(script.selection(), Some(1..1));
    assert!(
        script.caret_at(empty.origin.x.0 + ADVANCE),
        "the caret did not move on with the character that was typed: {:?}",
        script.carets()
    );
}
