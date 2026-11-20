//! A field that holds more text than it shows: the caret stays in view, and a drag past an edge
//! scrolls the field.
//!
//! The fixed face makes the numbers computable: one cluster is eight device pixels wide. The field
//! is 80 pixels wide and its content box 56, so it shows seven clusters. The padding leaves room for
//! the caret after the last cluster, as the padding of a real field does.

mod support;

use std::time::Duration;

use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Rect, Size};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{
    KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction, PointerEvent,
    ScrollDelta, ScrollPhase, Timestamp, WheelEvent,
};

/// The field starts 20 pixels in and 12 down, and only code scrolls it.
const HIDDEN: &str = "root { display: block; width: 400px; height: 300px; padding: 12px 20px }
    editor { display: flex; align-items: center; width: 80px; height: 40px; padding: 0 12px;
             overflow: hidden; white-space: pre }";

/// The same field, which a person can scroll too.
const AUTO: &str = "root { display: block; width: 400px; height: 300px; padding: 12px 20px }
    editor { display: flex; align-items: center; width: 80px; height: 40px; padding: 0 12px;
             overflow-x: auto; overflow-y: hidden; white-space: pre }";

/// The left and right edges of the field's padding box on the device, which is what it shows.
const LEFT: f32 = 20.0;
const RIGHT: f32 = 100.0;

/// Where the text starts when the field is not scrolled.
const TEXT: f32 = 32.0;

/// The vertical middle of the field.
const MIDDLE: f32 = 32.0;

/// Sixteen clusters, nine more than the field shows.
const LONG: &str = "abcdefghijklmnop";

struct Script {
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    editor: NodeRef,
}

impl Script {
    /// A window with one focused field that holds `content`.
    fn new(css: &'static str, content: &'static str) -> Self {
        let editor = NodeRef::new();
        let harness = support::app_with_text(css, move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(zgui_elements::editor().node_ref(editor).child(content))
                    .into_view()
                    .build(cx),
            )
        });
        let mut script = Self { harness, editor };
        script.harness.settle(8);
        script.key(NamedKey::Tab, KeyCode::Tab);
        script.harness.settle(8);
        script
    }

    fn window(&self) -> &zgui_runtime::Window {
        &self.harness.app().windows()[0]
    }

    fn key(&mut self, key: NamedKey, code: KeyCode) {
        self.harness.deliver_to_first(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(key, PhysicalKey::Code(code)),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }

    fn point(&mut self, action: PointerAction, x: f32) {
        self.harness.deliver_to_first(SurfaceEvent::Pointer {
            action,
            event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(MIDDLE))),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }

    /// Where the first line starts, which moves left as the field scrolls right.
    fn line_x(&self) -> f32 {
        support::first_line_on_device(self.window()).origin.x.0
    }

    /// The caret this frame drew.
    fn caret(&self) -> Rect<DevicePx, Device> {
        self.window()
            .caret_rect()
            .expect("a focused field draws a caret")
    }

    /// One frame, 16 ms after the last one.
    fn frame(&mut self) {
        self.harness.advance(Duration::from_millis(16));
        self.harness.pump();
    }
}

/// Whether a caret is inside the field.
fn shown(caret: Rect<DevicePx, Device>) -> bool {
    caret.left().0 >= LEFT && caret.right().0 <= RIGHT
}

#[test]
fn the_frame_that_moves_the_caret_past_the_edge_shows_it() {
    let mut script = Script::new(HIDDEN, LONG);
    script.key(NamedKey::End, KeyCode::End);
    assert_eq!(script.harness.pump(), 1);
    assert_eq!(script.editor.selection(), Some(16..16));
    assert!(
        shown(script.caret()),
        "the first frame after End drew the caret outside the field: {:?}",
        script.caret()
    );
    assert!(script.line_x() < TEXT, "the field did not scroll");

    script.key(NamedKey::Home, KeyCode::Home);
    script.harness.pump();
    assert_eq!(
        script.line_x(),
        TEXT,
        "Home scrolls the field back to its start"
    );
    assert!(shown(script.caret()));
}

#[test]
fn a_field_a_person_scrolled_stays_until_the_caret_moves() {
    let mut script = Script::new(AUTO, LONG);
    script.key(NamedKey::End, KeyCode::End);
    script.harness.settle(8);
    let revealed = script.line_x();

    script.harness.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(50.0), CssPx(MIDDLE)),
            delta: ScrollDelta::Pixels(Size::new(CssPx(-40.0), CssPx(0.0))),
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    for _ in 0..30 {
        script.frame();
    }
    script.harness.settle(8);
    assert_eq!(
        script.line_x(),
        revealed + 40.0,
        "the wheel scrolled the field back"
    );

    // The caret blinks and the window redraws. Nothing moves the field.
    script
        .harness
        .advance(zgui_runtime::caret::blink::HALF_PERIOD * 3);
    script.harness.settle(8);
    assert_eq!(script.line_x(), revealed + 40.0);

    script.key(NamedKey::ArrowLeft, KeyCode::ArrowLeft);
    script.harness.settle(8);
    assert_eq!(script.editor.selection(), Some(15..15));
    assert!(
        shown(script.caret()),
        "a caret move shows the caret again: {:?}",
        script.caret()
    );
}

#[test]
fn a_drag_past_the_edge_scrolls_at_the_pointers_speed_and_selects_what_comes_into_view() {
    let mut script = Script::new(HIDDEN, "abcdefghijklmnopqrstuvwxyz");
    script.point(PointerAction::Pressed, TEXT + 8.0 * 1.2);
    script.harness.settle(8);
    // Twenty pixels past the right edge.
    script.point(PointerAction::Moved, RIGHT + 20.0);
    script.harness.pump();
    let selection = script.editor.selection().expect("a drag selects");
    assert!(
        selection.end <= 9,
        "the drag selected text the field does not show: {selection:?}"
    );

    let start = script.line_x();
    let mut frames = 0;
    while script.editor.selection() != Some(1..26) {
        script.frame();
        frames += 1;
        // 21 pixels outside the port, at 12 pixels a second for each, is 252 pixels a second.
        let scrolled = start - script.line_x();
        let allowed = 252.0 * 0.016 * frames as f32 + 1.0;
        assert!(
            scrolled <= allowed,
            "{scrolled} pixels in {frames} frames is faster than the pointer asks"
        );
        assert!(
            frames < 200,
            "the drag never selected to the end of the text"
        );
    }
    assert!(
        frames > 20,
        "the field scrolled to its end in {frames} frames"
    );

    // At the end of the text, the drag stops asking for frames.
    script.frame();
    script.harness.settle(8);

    script.point(PointerAction::Released, RIGHT + 20.0);
    script.harness.settle(8);
    assert_eq!(script.editor.selection(), Some(1..26));
}
