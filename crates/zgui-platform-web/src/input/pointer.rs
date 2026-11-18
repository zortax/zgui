//! Mouse, touch and pen as one pointer stream.

use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::{PointerButton, PointerEvent, PointerId, PointerKind};

/// The position of a mouse-like event in the canvas, in CSS pixels.
pub fn position(event: &web_sys::MouseEvent) -> Point<CssPx, Css> {
    Point::new(
        CssPx(event.offset_x() as f32),
        CssPx(event.offset_y() as f32),
    )
}

/// The button a DOM button index names.
pub const fn button(index: i16) -> Option<PointerButton> {
    match index {
        0 => Some(PointerButton::Primary),
        1 => Some(PointerButton::Middle),
        2 => Some(PointerButton::Secondary),
        3 => Some(PointerButton::Back),
        4 => Some(PointerButton::Forward),
        other if other > 4 => Some(PointerButton::Other(other as u16)),
        _ => None,
    }
}

/// The kind of device a DOM pointer type names.
pub fn kind(pointer_type: &str) -> PointerKind {
    match pointer_type {
        "mouse" => PointerKind::Mouse,
        "touch" => PointerKind::Touch,
        "pen" => PointerKind::Pen,
        _ => PointerKind::Unknown,
    }
}

/// The identity of a pointer. The mouse is always [`PointerId::MOUSE`]; a contact keeps the
/// identifier the browser gave it, moved off zero.
pub fn id(kind: PointerKind, pointer_id: i32) -> PointerId {
    if kind == PointerKind::Mouse {
        PointerId::MOUSE
    } else {
        PointerId::new(u64::from(pointer_id.unsigned_abs()) + 1)
    }
}

/// The pointer event a DOM pointer event describes.
///
/// `pressed` says whether the event is a press or a release, which are the events that carry a
/// button. A contact always carries the primary button, because a touch has no other.
pub fn event(event: &web_sys::PointerEvent, pressed: bool) -> PointerEvent {
    let kind = kind(&event.pointer_type());
    let button = match kind {
        PointerKind::Mouse | PointerKind::Unknown => {
            if pressed {
                button(event.button())
            } else {
                None
            }
        }
        _ => Some(PointerButton::Primary),
    };
    PointerEvent {
        id: id(kind, event.pointer_id()),
        kind,
        primary: event.is_primary(),
        position: position(event),
        button,
        pressure: (kind != PointerKind::Mouse).then(|| event.pressure()),
    }
}
