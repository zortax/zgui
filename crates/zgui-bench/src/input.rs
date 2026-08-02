//! One event, as the platform would have delivered it.

use zgui::geom::{Css, CssPx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::runtime::Runtime;
use zgui::vocab::{
    KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction, PointerEvent,
    PointerId, PointerKind, ScrollDelta, ScrollPhase, Timestamp, WheelEvent,
};
use zgui_platform_headless::Harness;

/// One pointer event at `at`.
pub(crate) fn pointer(action: PointerAction, at: Point<CssPx, Css>) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(at),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// One wheel notch at `at`.
pub(crate) fn wheel(at: Point<CssPx, Css>, lines: f32) -> SurfaceEvent {
    SurfaceEvent::Wheel {
        event: WheelEvent {
            delta: ScrollDelta::Lines { x: 0.0, y: lines },
            phase: ScrollPhase::Discrete,
            position: at,
            id: PointerId::MOUSE,
            kind: PointerKind::Mouse,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// One move of a held scroll gesture, in CSS pixels rather than notches.
pub(crate) fn drag(at: Point<CssPx, Css>, pixels: f32, phase: ScrollPhase) -> SurfaceEvent {
    SurfaceEvent::Wheel {
        event: WheelEvent {
            delta: ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(pixels))),
            phase,
            position: at,
            id: PointerId::MOUSE,
            kind: PointerKind::Touch,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// One key going down.
pub(crate) fn key(event: KeyEvent) -> SurfaceEvent {
    SurfaceEvent::Key {
        state: KeyState::Pressed,
        event,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// One key coming back up.
pub(crate) fn key_up(event: KeyEvent) -> SurfaceEvent {
    SurfaceEvent::Key {
        state: KeyState::Released,
        event,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// Brings a fresh window up to the point where a character typed into it reaches a text field.
pub(crate) fn focus_a_field(harness: &mut Harness<Runtime>) -> usize {
    focus_a_field_timed(harness).0
}

/// The same traversal, with what each Tab press cost.
///
/// The traversal is not setup. Moving focus one stop changes one element's computed style, and a
/// document in which that costs the whole document is a document whose worst frames are all in
/// here — so a driver that ran this before starting its stopwatch was excluding the interesting
/// frames from every number it went on to take. The costs are in microseconds, one per press.
pub(crate) fn focus_a_field_timed(harness: &mut Harness<Runtime>) -> (usize, Vec<f64>) {
    harness.deliver_to_first(SurfaceEvent::Focused(true));
    harness.settle(64);
    let mut tabs = 0;
    let mut costs = Vec::new();
    while !typing(harness) && tabs < 400 {
        let started = std::time::Instant::now();
        for state in [key, key_up] {
            harness.deliver_to_first(state(KeyEvent::named(
                NamedKey::Tab,
                PhysicalKey::Code(KeyCode::Tab),
            )));
            harness.settle(64);
        }
        costs.push(started.elapsed().as_secs_f64() * 1e6);
        tabs += 1;
    }
    (tabs, costs)
}

/// Whether the window has told the surface that text is being typed.
///
/// The surface is told exactly when focus lands on something editable, so this is how the driver
/// knows the tab traversal has reached a real text field rather than a button.
pub(crate) fn typing(harness: &Harness<Runtime>) -> bool {
    harness
        .platform()
        .offscreens()
        .first()
        .and_then(|surface| surface.last_text_input())
        .flatten()
        .is_some()
}

/// Whether a smooth scroll is still on its way.
pub(crate) fn gliding(harness: &Harness<Runtime>) -> bool {
    harness.app().windows()[0].scroll().borrow().is_animating()
}
