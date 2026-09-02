//! What each kind of event carries.

pub mod key;

mod animation;
mod drop;
mod focus;
mod ime;
mod pointer;
mod scroll;
mod text;
mod value;
mod wheel;

use crate::event::kind::EventKind;

pub use crate::event::payload::animation::{
    AnimationEvent, AnimationPhase, Pseudo, TransitionEvent, TransitionPhase,
};
pub use crate::event::payload::drop::DropEvent;
pub use crate::event::payload::focus::{FocusCause, FocusEvent};
pub use crate::event::payload::ime::ImeEvent;
pub use crate::event::payload::key::{
    Key, KeyCode, KeyEvent, KeyLocation, KeyState, NamedKey, PhysicalKey, UnknownKeyCode,
    UnknownNamedKey,
};
pub use crate::event::payload::pointer::{
    PointerAction, PointerButton, PointerEvent, PointerId, PointerKind, PointerSample,
};
pub use crate::event::payload::scroll::ScrollEvent;
pub use crate::event::payload::text::TextEvent;
pub use crate::event::payload::value::{ValueChange, ValueEvent};
pub use crate::event::payload::wheel::{ScrollDelta, ScrollPhase, WheelEvent};

/// Which shape of payload an event carries, without the payload itself.
///
/// This exists so that a registration and a dispatch can be checked against each other without
/// having an event in hand — the two agree by construction, and this is what makes that
/// construction verifiable rather than asserted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum PayloadKind {
    /// A pointer event.
    Pointer,
    /// A scroll request from a wheel or a gesture.
    Wheel,
    /// A key press or release.
    Key,
    /// Text produced by the keyboard.
    Text,
    /// A stage of composition by an input method.
    Ime,
    /// Focus arriving or leaving.
    Focus,
    /// Content dropped from outside the window.
    Drop,
    /// A control's value changing.
    Value,
    /// A scrolling element's offset changing.
    Scroll,
    /// A stage in a declared animation's life.
    Animation,
    /// A stage in a transition's life.
    Transition,
}

/// What an event carries, whatever kind it is.
///
/// This is an open set. A backend that produces a class of event this vocabulary has no variant
/// for is a reason to add one, not a reason to reach around the enumeration, which is why it is
/// marked as extensible and why matching on it requires a fallback arm.
///
/// ```
/// use zgui_vocab::{Payload, PayloadKind, TextEvent};
///
/// let payload = Payload::Text(TextEvent::new("x"));
/// assert_eq!(payload.kind(), PayloadKind::Text);
/// assert!(payload.as_text().is_some());
/// assert!(payload.as_pointer().is_none());
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Payload {
    /// A pointer event.
    Pointer(PointerEvent),
    /// A scroll request from a wheel or a gesture.
    Wheel(WheelEvent),
    /// A key press or release.
    Key(KeyEvent),
    /// Text produced by the keyboard.
    Text(TextEvent),
    /// A stage of composition by an input method.
    Ime(ImeEvent),
    /// Focus arriving or leaving.
    Focus(FocusEvent),
    /// Content dropped from outside the window.
    Drop(DropEvent),
    /// A control's value changing.
    Value(ValueEvent),
    /// A scrolling element's offset changing.
    Scroll(ScrollEvent),
    /// A stage in a declared animation's life.
    Animation(AnimationEvent),
    /// A stage in a transition's life.
    Transition(TransitionEvent),
}

impl Payload {
    /// Which shape of payload this is.
    pub const fn kind(&self) -> PayloadKind {
        match self {
            Self::Pointer(_) => PayloadKind::Pointer,
            Self::Wheel(_) => PayloadKind::Wheel,
            Self::Key(_) => PayloadKind::Key,
            Self::Text(_) => PayloadKind::Text,
            Self::Ime(_) => PayloadKind::Ime,
            Self::Focus(_) => PayloadKind::Focus,
            Self::Drop(_) => PayloadKind::Drop,
            Self::Value(_) => PayloadKind::Value,
            Self::Scroll(_) => PayloadKind::Scroll,
            Self::Animation(_) => PayloadKind::Animation,
            Self::Transition(_) => PayloadKind::Transition,
        }
    }

    /// The pointer event, when this is one.
    pub const fn as_pointer(&self) -> Option<&PointerEvent> {
        match self {
            Self::Pointer(event) => Some(event),
            _ => None,
        }
    }

    /// The scroll request, when this is one.
    pub const fn as_wheel(&self) -> Option<&WheelEvent> {
        match self {
            Self::Wheel(event) => Some(event),
            _ => None,
        }
    }

    /// The key event, when this is one.
    pub const fn as_key(&self) -> Option<&KeyEvent> {
        match self {
            Self::Key(event) => Some(event),
            _ => None,
        }
    }

    /// The text event, when this is one.
    pub const fn as_text(&self) -> Option<&TextEvent> {
        match self {
            Self::Text(event) => Some(event),
            _ => None,
        }
    }

    /// The composition event, when this is one.
    pub const fn as_ime(&self) -> Option<&ImeEvent> {
        match self {
            Self::Ime(event) => Some(event),
            _ => None,
        }
    }

    /// The focus event, when this is one.
    pub const fn as_focus(&self) -> Option<&FocusEvent> {
        match self {
            Self::Focus(event) => Some(event),
            _ => None,
        }
    }

    /// The drop event, when this is one.
    pub const fn as_drop(&self) -> Option<&DropEvent> {
        match self {
            Self::Drop(event) => Some(event),
            _ => None,
        }
    }

    /// The value change, when this is one.
    pub const fn as_value(&self) -> Option<&ValueEvent> {
        match self {
            Self::Value(event) => Some(event),
            _ => None,
        }
    }

    /// The scroll report, when this is one.
    pub const fn as_scroll(&self) -> Option<&ScrollEvent> {
        match self {
            Self::Scroll(event) => Some(event),
            _ => None,
        }
    }

    /// The animation lifecycle event, when this is one.
    pub const fn as_animation(&self) -> Option<&AnimationEvent> {
        match self {
            Self::Animation(event) => Some(event),
            _ => None,
        }
    }

    /// The transition lifecycle event, when this is one.
    pub const fn as_transition(&self) -> Option<&TransitionEvent> {
        match self {
            Self::Transition(event) => Some(event),
            _ => None,
        }
    }

    /// Whether this payload is the shape an event of `kind` carries.
    ///
    /// A dispatch that hands a handler the wrong payload shape is a bug that cannot be recovered
    /// from at the point it is noticed, so it is checked where the two meet.
    ///
    /// ```
    /// use zgui_vocab::{EventKind, Payload, TextEvent};
    ///
    /// let payload = Payload::Text(TextEvent::new("x"));
    /// assert!(payload.matches(EventKind::Text));
    /// assert!(!payload.matches(EventKind::Click));
    /// ```
    pub const fn matches(&self, kind: EventKind) -> bool {
        self.kind() as u8 == kind.payload_kind() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::{Payload, PayloadKind};
    use crate::event::kind::EventKind;
    use crate::event::payload::pointer::PointerEvent;
    use crate::event::payload::text::TextEvent;
    use zgui_geom::{CssPx, Point};

    #[test]
    fn a_payload_reports_its_own_shape() {
        let pointer = Payload::Pointer(PointerEvent::mouse(Point::new(CssPx(0.0), CssPx(0.0))));
        assert_eq!(pointer.kind(), PayloadKind::Pointer);
        assert!(pointer.as_pointer().is_some());
        assert!(pointer.as_key().is_none());
    }

    #[test]
    fn every_event_kind_agrees_with_the_payload_it_names() {
        let payload = Payload::Text(TextEvent::new("x"));
        for kind in EventKind::ALL {
            assert_eq!(
                payload.matches(*kind),
                kind.payload_kind() == PayloadKind::Text,
                "{kind:?} disagreed"
            );
        }
    }
}
