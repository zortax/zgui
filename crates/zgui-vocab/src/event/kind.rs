//! Which event happened.

use core::fmt::{self, Display};
use core::str::FromStr;

use crate::event::payload::PayloadKind;

/// Declares one variant per event, with everything the dispatcher needs to know about it.
macro_rules! events {
    ($(
        $name:ident => $text:literal, $web:literal, $payload:ident,
        bubbles: $bubbles:literal, cancelable: $cancelable:literal, $doc:literal;
    )+) => {
        /// Which event happened.
        ///
        /// This is the runtime name an event is registered and dispatched under. It is a closed
        /// enumeration rather than an interned string because a listener's registration has to be
        /// a compile-time constant: an event's identity is what lets a handler's argument type be
        /// inferred from the event it is registered for.
        ///
        /// ```
        /// use zgui_vocab::EventKind;
        ///
        /// assert_eq!(EventKind::PointerDown.name(), "pointer_down");
        /// assert_eq!(EventKind::PointerDown.web_name(), "pointerdown");
        /// assert!(EventKind::Click.bubbles());
        /// assert!(!EventKind::Scroll.bubbles());
        /// ```
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[non_exhaustive]
        pub enum EventKind {
            $(
                #[doc = $doc]
                $name,
            )+
        }

        impl EventKind {
            /// Every event, in declaration order.
            pub const ALL: &'static [Self] = &[ $( Self::$name, )+ ];

            /// The name this event is written with.
            pub const fn name(self) -> &'static str {
                match self {
                    $( Self::$name => $text, )+
                }
            }

            /// The name a document-object-model backend registers this event under.
            ///
            /// Every event in this vocabulary has one, which is what keeps the whole set
            /// expressible on a backend that is not this framework's own. Two events may share
            /// one where the web draws no distinction.
            pub const fn web_name(self) -> &'static str {
                match self {
                    $( Self::$name => $web, )+
                }
            }

            /// Which payload an event of this kind carries.
            pub const fn payload_kind(self) -> PayloadKind {
                match self {
                    $( Self::$name => PayloadKind::$payload, )+
                }
            }

            /// Whether this event travels up from its target to the root.
            ///
            /// An event that does not bubble is delivered to its target only, so a listener on an
            /// ancestor never sees it.
            pub const fn bubbles(self) -> bool {
                match self {
                    $( Self::$name => $bubbles, )+
                }
            }

            /// Whether a handler can suppress this event's default behaviour.
            ///
            /// Suppressing what cannot be suppressed silently does nothing, so the answer belongs
            /// with the event rather than in each handler's head.
            pub const fn is_cancelable(self) -> bool {
                match self {
                    $( Self::$name => $cancelable, )+
                }
            }
        }

        impl FromStr for EventKind {
            type Err = UnknownEventKind;

            fn from_str(text: &str) -> Result<Self, UnknownEventKind> {
                match text {
                    $( $text => Ok(Self::$name), )+
                    _ => Err(UnknownEventKind),
                }
            }
        }
    };
}

/// The error from parsing a name that is not an event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnknownEventKind;

impl Display for UnknownEventKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("not an event name")
    }
}

impl core::error::Error for UnknownEventKind {}

events! {
    PointerDown => "pointer_down", "pointerdown", Pointer,
        bubbles: true, cancelable: true,
        "A pointer button went down over the element.";
    PointerUp => "pointer_up", "pointerup", Pointer,
        bubbles: true, cancelable: true,
        "A pointer button came up over the element.";
    PointerMove => "pointer_move", "pointermove", Pointer,
        bubbles: true, cancelable: true,
        "A pointer moved while over the element.";
    PointerEnter => "pointer_enter", "pointerenter", Pointer,
        bubbles: false, cancelable: false,
        "A pointer moved onto the element or one of its descendants.";
    PointerLeave => "pointer_leave", "pointerleave", Pointer,
        bubbles: false, cancelable: false,
        "A pointer moved off the element and all of its descendants.";
    PointerCancel => "pointer_cancel", "pointercancel", Pointer,
        bubbles: true, cancelable: false,
        "An interaction was taken over by something else and will produce no release.";
    Click => "click", "click", Pointer,
        bubbles: true, cancelable: true,
        "The element was activated by a primary press and release.";
    DoubleClick => "double_click", "dblclick", Pointer,
        bubbles: true, cancelable: true,
        "The element was activated twice in quick succession.";
    ContextMenu => "context_menu", "contextmenu", Pointer,
        bubbles: true, cancelable: true,
        "The element was asked for its context menu.";

    Wheel => "wheel", "wheel", Wheel,
        bubbles: true, cancelable: true,
        "A wheel or scroll gesture asked to scroll over the element.";

    KeyDown => "key_down", "keydown", Key,
        bubbles: true, cancelable: true,
        "A key went down while the element had focus.";
    KeyUp => "key_up", "keyup", Key,
        bubbles: true, cancelable: true,
        "A key came up while the element had focus.";
    Text => "text", "beforeinput", Text,
        bubbles: true, cancelable: true,
        "The keyboard produced text to insert at the caret.";

    ImeStart => "ime_start", "compositionstart", Ime,
        bubbles: true, cancelable: true,
        "An input method took over and may produce provisional text.";
    ImePreedit => "ime_preedit", "compositionupdate", Ime,
        bubbles: true, cancelable: false,
        "An input method's provisional text changed.";
    ImeCommit => "ime_commit", "compositionend", Ime,
        bubbles: true, cancelable: false,
        "An input method finished a composition and the text is now real.";
    ImeEnd => "ime_end", "compositionend", Ime,
        bubbles: true, cancelable: false,
        "An input method let go, abandoning any provisional text.";

    FocusIn => "focus_in", "focusin", Focus,
        bubbles: true, cancelable: false,
        "Focus arrived at the element or one of its descendants.";
    FocusOut => "focus_out", "focusout", Focus,
        bubbles: true, cancelable: false,
        "Focus left the element and all of its descendants.";

    Drop => "drop", "drop", Drop,
        bubbles: true, cancelable: true,
        "Content from outside the window was dropped on the element.";

    Input => "input", "input", Value,
        bubbles: true, cancelable: false,
        "The element's value changed as the user is working.";
    Change => "change", "change", Value,
        bubbles: true, cancelable: false,
        "The user settled on the element's value.";
    Scroll => "scroll", "scroll", Scroll,
        bubbles: false, cancelable: false,
        "The element's scroll offset changed.";

    AnimationStart => "animation_start", "animationstart", Animation,
        bubbles: true, cancelable: false,
        "A declared animation on the element began.";
    AnimationIteration => "animation_iteration", "animationiteration", Animation,
        bubbles: true, cancelable: false,
        "A declared animation on the element finished one iteration and began another.";
    AnimationEnd => "animation_end", "animationend", Animation,
        bubbles: true, cancelable: false,
        "A declared animation on the element finished on its own.";
    AnimationCancel => "animation_cancel", "animationcancel", Animation,
        bubbles: true, cancelable: false,
        "A declared animation on the element was stopped before it finished.";

    TransitionRun => "transition_run", "transitionrun", Transition,
        bubbles: true, cancelable: false,
        "A transition on the element was created and is waiting out its delay.";
    TransitionStart => "transition_start", "transitionstart", Transition,
        bubbles: true, cancelable: false,
        "A transition on the element began moving its value.";
    TransitionEnd => "transition_end", "transitionend", Transition,
        bubbles: true, cancelable: false,
        "A transition on the element reached its destination.";
    TransitionCancel => "transition_cancel", "transitioncancel", Transition,
        bubbles: true, cancelable: false,
        "A transition on the element was stopped before its value arrived.";
}

impl Display for EventKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::{EventKind, UnknownEventKind};
    use crate::event::payload::PayloadKind;
    use core::str::FromStr;

    #[test]
    fn every_event_round_trips_through_its_name() {
        for kind in EventKind::ALL {
            assert_eq!(EventKind::from_str(kind.name()), Ok(*kind));
        }
    }

    #[test]
    fn no_two_events_share_a_name() {
        for (index, kind) in EventKind::ALL.iter().enumerate() {
            for other in &EventKind::ALL[index + 1..] {
                assert_ne!(kind.name(), other.name(), "{kind:?} and {other:?} collide");
            }
        }
    }

    #[test]
    fn every_event_has_a_document_object_model_name() {
        for kind in EventKind::ALL {
            assert!(!kind.web_name().is_empty(), "{kind:?} has no web name");
            assert!(
                kind.web_name()
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                "{kind:?}'s web name is not a plain event name"
            );
        }
    }

    #[test]
    fn an_unknown_name_is_reported_rather_than_guessed() {
        assert_eq!(EventKind::from_str("pointerdown"), Err(UnknownEventKind));
        assert_eq!(EventKind::from_str(""), Err(UnknownEventKind));
    }

    #[test]
    fn the_events_that_must_not_bubble_do_not() {
        for kind in [
            EventKind::Scroll,
            EventKind::PointerEnter,
            EventKind::PointerLeave,
        ] {
            assert!(!kind.bubbles(), "{kind:?} must not bubble");
        }
        assert!(EventKind::Click.bubbles());
    }

    #[test]
    fn a_report_of_something_already_done_is_never_cancelable() {
        for kind in [
            EventKind::Scroll,
            EventKind::Input,
            EventKind::Change,
            EventKind::AnimationEnd,
            EventKind::TransitionEnd,
            EventKind::FocusIn,
        ] {
            assert!(!kind.is_cancelable(), "{kind:?} must not be cancelable");
        }
    }

    #[test]
    fn every_pointer_event_carries_a_pointer_payload() {
        for kind in EventKind::ALL {
            if kind.name().starts_with("pointer_") || *kind == EventKind::Click {
                assert_eq!(kind.payload_kind(), PayloadKind::Pointer);
            }
        }
    }
}
