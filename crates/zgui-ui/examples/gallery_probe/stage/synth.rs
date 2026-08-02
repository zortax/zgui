//! The events a driver makes, in the shape the windowing backend would have made them.
//!
//! Every constructor here fills the same fields a real backend fills, because a control that
//! branches on a field the driver left at its default is a control that has not been driven. The
//! positions are in CSS pixels from the window's corner, which is the unit the platform reports
//! after dividing by the output's scale.

use zgui::geom::{Css, CssPx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::vocab::{
    KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction, PointerButton,
    PointerEvent, ScrollDelta, ScrollPhase, Timestamp, WheelEvent,
};

/// A pointer doing `action` at `at`, with `modifiers` held.
pub(crate) fn pointer(
    action: PointerAction,
    at: Point<CssPx, Css>,
    button: Option<PointerButton>,
    modifiers: Modifiers,
    when: Timestamp,
) -> SurfaceEvent {
    let mut event = PointerEvent::mouse(at);
    if let Some(button) = button {
        event = event.with_button(button);
    }
    SurfaceEvent::Pointer {
        action,
        event,
        modifiers,
        timestamp: when,
    }
}

/// A wheel notch of `lines` at `at`.
pub(crate) fn wheel(
    at: Point<CssPx, Css>,
    lines: (f32, f32),
    modifiers: Modifiers,
    when: Timestamp,
) -> SurfaceEvent {
    SurfaceEvent::Wheel {
        event: WheelEvent {
            delta: ScrollDelta::Lines {
                x: lines.0,
                y: lines.1,
            },
            phase: ScrollPhase::Discrete,
            position: at,
            id: zgui::vocab::PointerId::MOUSE,
            kind: zgui::vocab::PointerKind::Mouse,
        },
        modifiers,
        timestamp: when,
    }
}

/// A trackpad scroll of `pixels` at `at`, in the middle of a gesture.
pub(crate) fn trackpad(
    at: Point<CssPx, Css>,
    pixels: Size<CssPx, Css>,
    phase: ScrollPhase,
    when: Timestamp,
) -> SurfaceEvent {
    SurfaceEvent::Wheel {
        event: WheelEvent {
            delta: ScrollDelta::Pixels(pixels),
            phase,
            position: at,
            id: zgui::vocab::PointerId::MOUSE,
            kind: zgui::vocab::PointerKind::Mouse,
        },
        modifiers: Modifiers::NONE,
        timestamp: when,
    }
}

/// A named key going `state`, with `modifiers` held.
pub(crate) fn named(
    key: NamedKey,
    state: KeyState,
    modifiers: Modifiers,
    when: Timestamp,
) -> SurfaceEvent {
    SurfaceEvent::Key {
        state,
        event: KeyEvent::named(key, physical_for(key)),
        modifiers,
        timestamp: when,
    }
}

/// A key that produces `text` going `state`.
pub(crate) fn character(
    text: &str,
    state: KeyState,
    modifiers: Modifiers,
    when: Timestamp,
) -> SurfaceEvent {
    let key = zgui::vocab::Key::character(text);
    SurfaceEvent::Key {
        state,
        event: KeyEvent {
            key: key.clone(),
            key_without_modifiers: key,
            physical: physical_for_character(text),
            location: zgui::vocab::KeyLocation::Standard,
            repeat: false,
        },
        modifiers,
        timestamp: when,
    }
}

/// Where a named key sits on the keyboard.
///
/// The layout-independent half of a key event is not decoration: a shortcut resolved against the
/// physical key would never fire for an event that left it unidentified, so the driver fills it in
/// for the keys it presses rather than sending a key that is nowhere.
fn physical_for(key: NamedKey) -> PhysicalKey {
    let code = match key {
        NamedKey::Enter => KeyCode::Enter,
        NamedKey::Tab => KeyCode::Tab,
        NamedKey::Space => KeyCode::Space,
        NamedKey::Escape => KeyCode::Escape,
        NamedKey::Backspace => KeyCode::Backspace,
        NamedKey::Delete => KeyCode::Delete,
        NamedKey::ArrowUp => KeyCode::ArrowUp,
        NamedKey::ArrowDown => KeyCode::ArrowDown,
        NamedKey::ArrowLeft => KeyCode::ArrowLeft,
        NamedKey::ArrowRight => KeyCode::ArrowRight,
        NamedKey::Home => KeyCode::Home,
        NamedKey::End => KeyCode::End,
        NamedKey::PageUp => KeyCode::PageUp,
        NamedKey::PageDown => KeyCode::PageDown,
        _ => return PhysicalKey::Unidentified(0),
    };
    PhysicalKey::Code(code)
}

/// Where the key that produces `text` sits, for the letters and digits a driver types.
fn physical_for_character(text: &str) -> PhysicalKey {
    let mut characters = text.chars();
    let (Some(first), None) = (characters.next(), characters.next()) else {
        return PhysicalKey::Unidentified(0);
    };
    let code = match first.to_ascii_lowercase() {
        'a' => KeyCode::KeyA,
        'b' => KeyCode::KeyB,
        'c' => KeyCode::KeyC,
        'd' => KeyCode::KeyD,
        'e' => KeyCode::KeyE,
        'f' => KeyCode::KeyF,
        'g' => KeyCode::KeyG,
        'h' => KeyCode::KeyH,
        'i' => KeyCode::KeyI,
        'j' => KeyCode::KeyJ,
        'k' => KeyCode::KeyK,
        'l' => KeyCode::KeyL,
        'm' => KeyCode::KeyM,
        'n' => KeyCode::KeyN,
        'o' => KeyCode::KeyO,
        'p' => KeyCode::KeyP,
        'q' => KeyCode::KeyQ,
        'r' => KeyCode::KeyR,
        's' => KeyCode::KeyS,
        't' => KeyCode::KeyT,
        'u' => KeyCode::KeyU,
        'v' => KeyCode::KeyV,
        'w' => KeyCode::KeyW,
        'x' => KeyCode::KeyX,
        'y' => KeyCode::KeyY,
        'z' => KeyCode::KeyZ,
        '0' => KeyCode::Digit0,
        '1' => KeyCode::Digit1,
        '2' => KeyCode::Digit2,
        '3' => KeyCode::Digit3,
        '4' => KeyCode::Digit4,
        '5' => KeyCode::Digit5,
        '6' => KeyCode::Digit6,
        '7' => KeyCode::Digit7,
        '8' => KeyCode::Digit8,
        '9' => KeyCode::Digit9,
        '@' => KeyCode::Digit2,
        '.' => KeyCode::Period,
        ',' => KeyCode::Comma,
        '-' => KeyCode::Minus,
        ' ' => KeyCode::Space,
        _ => return PhysicalKey::Unidentified(0),
    };
    PhysicalKey::Code(code)
}
