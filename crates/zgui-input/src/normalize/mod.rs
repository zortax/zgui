//! What the window said, in units a document can use.
//!
//! A windowing system reports what its hardware gave it: a wheel notch as a count of lines, a
//! pointer position in the units the surface was configured with, a key repeat that is a repeat
//! for one purpose and not for another. None of that is wrong, and none of it is usable without
//! being told what it means for *this* document — how tall a line is on the element being
//! scrolled, what the surface's scale is, whether the key is being read as a command or as text.
//!
//! Every conversion that needs such a fact takes it as an argument. That is the whole design rule
//! of this module: nothing here invents a constant, because a constant invented here is a scroll
//! that moves the wrong distance on every document whose text is not the size the constant
//! assumed.

pub mod keyboard;
pub mod pointer;
pub mod scroll;

use zgui_platform::SurfaceEvent;
use zgui_vocab::{EventKind, Modifiers, Payload, Timestamp};

pub use crate::normalize::scroll::ScrollUnits;

/// One thing a person did, ready to be routed into a document.
///
/// The kind and the payload agree by construction, because both come from the same platform
/// event, and the modifiers and the timestamp travel with them because a handler needs all four
/// and matching on the platform's own enumeration a second time to recover the last two is how
/// they drift apart.
#[derive(Clone, Debug, PartialEq)]
pub struct InputEvent {
    /// Which event this is.
    pub kind: EventKind,
    /// What it carries.
    pub payload: Payload,
    /// Which modifier keys were held when it happened.
    pub modifiers: Modifiers,
    /// When it happened.
    pub timestamp: Timestamp,
}

impl InputEvent {
    /// The document event a surface event is, or [`None`] when it is not one.
    ///
    /// A surface event that changes state without being an occurrence — the held modifiers
    /// changing, a drag merely passing over — answers with nothing, because dispatching one as an
    /// event would deliver a drop that never happened.
    ///
    /// `held` supplies the modifiers for the events the platform does not stamp with them, which
    /// is what the running modifier state is kept for.
    ///
    /// ```
    /// use zgui_geom::{CssPx, Point};
    /// use zgui_input::normalize::InputEvent;
    /// use zgui_platform::SurfaceEvent;
    /// use zgui_vocab::{EventKind, Modifiers, PointerAction, PointerEvent, Timestamp};
    ///
    /// let pressed = SurfaceEvent::Pointer {
    ///     action: PointerAction::Pressed,
    ///     event: PointerEvent::mouse(Point::new(CssPx(1.0), CssPx(2.0))),
    ///     modifiers: Modifiers::SHIFT,
    ///     timestamp: Timestamp::ORIGIN,
    /// };
    ///
    /// let event = InputEvent::of(&pressed, Modifiers::NONE).expect("a press is an occurrence");
    /// assert_eq!(event.kind, EventKind::PointerDown);
    /// assert_eq!(event.modifiers, Modifiers::SHIFT);
    ///
    /// assert!(InputEvent::of(&SurfaceEvent::Occluded(true), Modifiers::NONE).is_none());
    /// ```
    pub fn of(event: &SurfaceEvent, held: Modifiers) -> Option<Self> {
        let (kind, payload) = event.to_dispatch()?;
        Some(Self {
            kind,
            payload,
            modifiers: event.modifiers().unwrap_or(held),
            timestamp: event.timestamp().unwrap_or(Timestamp::ORIGIN),
        })
    }
}

/// The modifier keys currently held, kept across events.
///
/// A modifier can change while the surface has no keyboard focus, so a set recovered only from key
/// events is wrong until the next press — which is why the platform reports the change on its own
/// and why this exists to remember it.
///
/// ```
/// use zgui_input::normalize::HeldModifiers;
/// use zgui_platform::SurfaceEvent;
/// use zgui_vocab::Modifiers;
///
/// let mut held = HeldModifiers::default();
/// assert_eq!(held.get(), Modifiers::NONE);
///
/// held.observe(&SurfaceEvent::ModifiersChanged(Modifiers::CONTROL));
/// assert_eq!(held.get(), Modifiers::CONTROL);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeldModifiers(Modifiers);

impl HeldModifiers {
    /// What is held now.
    pub fn get(self) -> Modifiers {
        self.0
    }

    /// Takes the held set from any event that reports one.
    ///
    /// Every stamped input event carries the set as well as the change, so this follows the
    /// platform rather than accumulating presses and releases of its own — an accumulator loses
    /// track the first time a modifier is released while the window is not focused.
    pub fn observe(&mut self, event: &SurfaceEvent) {
        if let Some(modifiers) = event.modifiers() {
            self.0 = modifiers;
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{CssPx, Point};
    use zgui_platform::SurfaceEvent;
    use zgui_vocab::{
        EventKind, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction,
        PointerEvent, Timestamp,
    };

    use super::{HeldModifiers, InputEvent};

    #[test]
    fn an_unstamped_event_takes_the_modifiers_that_are_held() {
        // An input method's commit carries no modifiers of its own; the set that was held when it
        // arrived is still what a handler has to be told.
        let ime = SurfaceEvent::Ime(zgui_vocab::ImeEvent::Commit("x".into()));
        let event = InputEvent::of(&ime, Modifiers::ALT).expect("a commit is an occurrence");
        assert_eq!(event.modifiers, Modifiers::ALT);
        assert_eq!(event.timestamp, Timestamp::ORIGIN);
    }

    #[test]
    fn a_change_in_the_held_set_is_remembered_and_dispatches_nothing() {
        let mut held = HeldModifiers::default();
        let change = SurfaceEvent::ModifiersChanged(Modifiers::CONTROL | Modifiers::SHIFT);
        held.observe(&change);
        assert_eq!(held.get(), Modifiers::CONTROL | Modifiers::SHIFT);
        assert!(InputEvent::of(&change, Modifiers::NONE).is_none());
    }

    #[test]
    fn a_stamped_event_updates_the_held_set_too() {
        let mut held = HeldModifiers::default();
        held.observe(&SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter)),
            modifiers: Modifiers::META,
            timestamp: Timestamp::ORIGIN,
        });
        assert_eq!(held.get(), Modifiers::META);
    }

    #[test]
    fn every_pointer_action_becomes_its_own_event() {
        // Every variant, not a sample. `Entered` and `Left` are the two this list was once
        // missing, and they are the two a hover is made of: a pointer arriving over a control and
        // leaving it again is the whole of `:hover`, so a normaliser that dropped them would take
        // every hover style in every document with it while every other assertion here stayed
        // green. `PointerAction` is `#[non_exhaustive]`, so no match in this crate can be made to
        // fail on a new variant; `zgui_vocab`'s own `every_action_is_covered_here` is what does
        // that, and it names this test as the list to extend.
        for (action, kind) in [
            (PointerAction::Entered, EventKind::PointerEnter),
            (PointerAction::Moved, EventKind::PointerMove),
            (PointerAction::Pressed, EventKind::PointerDown),
            (PointerAction::Released, EventKind::PointerUp),
            (PointerAction::Left, EventKind::PointerLeave),
            (PointerAction::Cancelled, EventKind::PointerCancel),
        ] {
            let event = SurfaceEvent::Pointer {
                action,
                event: PointerEvent::mouse(Point::new(CssPx(0.0), CssPx(0.0))),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            };
            assert_eq!(
                InputEvent::of(&event, Modifiers::NONE)
                    .expect("dispatchable")
                    .kind,
                kind
            );
        }
    }
}
