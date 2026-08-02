//! One type, and one constant, per event.

use zgui_vocab::{
    AnimationEvent, DropEvent, EventKind, FocusEvent, ImeEvent, KeyEvent, Payload, PointerEvent,
    ScrollEvent, TextEvent, TransitionEvent, ValueEvent, WheelEvent,
};

use crate::event::{EventType, EventView};

/// Declares the zero-sized type and the constant for each event.
macro_rules! events {
    ($( $constant:ident : $type_name:ident => $kind:ident, $payload:ty, $extract:ident, $doc:literal; )+) => {
        $(
            #[doc = $doc]
            ///
            /// The type exists so that a handler registered for this event knows what its argument
            /// is without being told. Use the constant of the same name in lower case.
            #[derive(Copy, Clone, PartialEq, Eq, Debug)]
            pub struct $type_name;

            #[doc = $doc]
            pub const $constant: $type_name = $type_name;

            impl EventView for $type_name {
                type Payload = $payload;

                fn view(payload: &Payload) -> &Self::Payload {
                    payload.$extract().unwrap_or_else(|| {
                        panic!(
                            "a listener registered for {} was handed a {:?} payload",
                            EventKind::$kind.name(),
                            payload.kind()
                        )
                    })
                }
            }

            impl EventType for $type_name {
                const KIND: EventKind = EventKind::$kind;
            }
        )+

        /// Every event that has a constant, paired with it.
        ///
        /// The list is what the completeness test reads; nothing else needs it.
        #[cfg(test)]
        pub(crate) const DECLARED: &[EventKind] = &[ $( EventKind::$kind, )+ ];
    };
}

events! {
    POINTER_DOWN: PointerDown => PointerDown, PointerEvent, as_pointer,
        "A pointer button went down over the element.";
    POINTER_UP: PointerUp => PointerUp, PointerEvent, as_pointer,
        "A pointer button came up over the element.";
    POINTER_MOVE: PointerMove => PointerMove, PointerEvent, as_pointer,
        "A pointer moved while over the element.";
    POINTER_ENTER: PointerEnter => PointerEnter, PointerEvent, as_pointer,
        "A pointer moved onto the element or one of its descendants.";
    POINTER_LEAVE: PointerLeave => PointerLeave, PointerEvent, as_pointer,
        "A pointer moved off the element and all of its descendants.";
    POINTER_CANCEL: PointerCancel => PointerCancel, PointerEvent, as_pointer,
        "An interaction was taken over by something else and will produce no release.";
    CLICK: Click => Click, PointerEvent, as_pointer,
        "The element was activated by a primary press and release.";
    DOUBLE_CLICK: DoubleClick => DoubleClick, PointerEvent, as_pointer,
        "The element was activated twice in quick succession.";
    CONTEXT_MENU: ContextMenu => ContextMenu, PointerEvent, as_pointer,
        "The element was asked for its context menu.";

    WHEEL: Wheel => Wheel, WheelEvent, as_wheel,
        "A wheel or scroll gesture asked to scroll over the element.";

    KEY_DOWN: KeyDown => KeyDown, KeyEvent, as_key,
        "A key went down while the element had focus.";
    KEY_UP: KeyUp => KeyUp, KeyEvent, as_key,
        "A key came up while the element had focus.";
    TEXT: Text => Text, TextEvent, as_text,
        "The keyboard produced text to insert at the caret.";

    IME_START: ImeStart => ImeStart, ImeEvent, as_ime,
        "An input method took over and may produce provisional text.";
    IME_PREEDIT: ImePreedit => ImePreedit, ImeEvent, as_ime,
        "An input method's provisional text changed.";
    IME_COMMIT: ImeCommit => ImeCommit, ImeEvent, as_ime,
        "An input method finished a composition and the text is now real.";
    IME_END: ImeEnd => ImeEnd, ImeEvent, as_ime,
        "An input method let go, abandoning any provisional text.";

    FOCUS_IN: FocusIn => FocusIn, FocusEvent, as_focus,
        "Focus arrived at the element or one of its descendants.";
    FOCUS_OUT: FocusOut => FocusOut, FocusEvent, as_focus,
        "Focus left the element and all of its descendants.";

    DROP: Drop => Drop, DropEvent, as_drop,
        "Content from outside the window was dropped on the element.";

    INPUT: Input => Input, ValueEvent, as_value,
        "The element's value changed as the user is working.";
    CHANGE: Change => Change, ValueEvent, as_value,
        "The user settled on the element's value.";
    SCROLL: Scroll => Scroll, ScrollEvent, as_scroll,
        "The element's scroll offset changed.";

    ANIMATION_START: AnimationStart => AnimationStart, AnimationEvent, as_animation,
        "A declared animation on the element began.";
    ANIMATION_ITERATION: AnimationIteration => AnimationIteration, AnimationEvent, as_animation,
        "A declared animation on the element finished one iteration and began another.";
    ANIMATION_END: AnimationEnd => AnimationEnd, AnimationEvent, as_animation,
        "A declared animation on the element finished on its own.";
    ANIMATION_CANCEL: AnimationCancel => AnimationCancel, AnimationEvent, as_animation,
        "A declared animation on the element was stopped before it finished.";

    TRANSITION_RUN: TransitionRun => TransitionRun, TransitionEvent, as_transition,
        "A transition on the element was created and is waiting out its delay.";
    TRANSITION_START: TransitionStart => TransitionStart, TransitionEvent, as_transition,
        "A transition on the element began changing its property.";
    TRANSITION_END: TransitionEnd => TransitionEnd, TransitionEvent, as_transition,
        "A transition on the element finished on its own.";
    TRANSITION_CANCEL: TransitionCancel => TransitionCancel, TransitionEvent, as_transition,
        "A transition on the element was stopped before it finished.";
}

#[cfg(test)]
mod tests {
    use zgui_vocab::EventKind;

    use super::{CLICK, DECLARED, KEY_DOWN};
    use crate::event::EventType;

    #[test]
    fn every_event_in_the_vocabulary_has_a_constant() {
        let missing: Vec<&str> = EventKind::ALL
            .iter()
            .filter(|kind| !DECLARED.contains(kind))
            .map(|kind| kind.name())
            .collect();
        assert!(
            missing.is_empty(),
            "these events have no constant in `zgui_view::events`: {missing:?}"
        );
    }

    #[test]
    fn no_two_constants_name_the_same_event() {
        let mut kinds = DECLARED.to_vec();
        kinds.sort();
        kinds.dedup();
        assert_eq!(kinds.len(), DECLARED.len());
    }

    #[test]
    fn a_set_of_events_is_built_from_kinds_because_the_constants_have_distinct_types() {
        // `[CLICK, KEY_DOWN]` is not an array: the distinctness that makes a handler's argument
        // inferable is exactly what makes a mixed literal ill-typed.
        let set = [CLICK.kind(), KEY_DOWN.kind()];
        assert_eq!(set, [EventKind::Click, EventKind::KeyDown]);
    }
}
