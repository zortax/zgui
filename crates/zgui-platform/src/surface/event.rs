//! Everything that can happen to one surface.

use zgui_geom::{Device, DevicePx, Size};
use zgui_vocab::{
    DropEvent, EventKind, ImeEvent, KeyEvent, KeyState, Modifiers, Payload, PointerAction,
    PointerEvent, Timestamp, WheelEvent,
};

use crate::surface::drag::DragEvent;
use crate::theme::ColorScheme;

/// Something that happened to a surface.
///
/// # Why input arrives as one pointer stream
///
/// A mouse, a finger and a stylus are one kind of event here, told apart by a field rather than by
/// which variant they arrive in. That is a deliberate constraint on everything above: a control
/// written against these events works under touch without being written a second time, and the
/// framework cannot grow a mouse-only path by accident because there is no mouse-only event to
/// grow it on.
///
/// # Why a drag is one event with a set of paths
///
/// The same reason, one layer up: a drop target told about files one at a time cannot know when
/// the set is complete.
#[derive(Debug)]
#[non_exhaustive]
pub enum SurfaceEvent {
    /// The surface's size changed, in physical pixels.
    Resized(Size<DevicePx, Device>),
    /// The scale the surface is presented at changed.
    ///
    /// The new size arrives with it, because a scale change and the resize it causes are one
    /// event as far as anything that has to redraw is concerned.
    ScaleFactorChanged {
        /// The new scale.
        scale_factor: f64,
        /// The surface's size at the new scale, in physical pixels.
        size: Size<DevicePx, Device>,
    },
    /// The surface should be drawn.
    ///
    /// This arrives only in answer to a redraw request. Nothing else produces it — not a reached
    /// deadline, not a wake — so anything that wants a frame has to ask for one.
    RedrawRequested,
    /// The user asked for the surface to close. Nothing has closed yet.
    CloseRequested,
    /// The surface is gone.
    Destroyed,
    /// The surface gained or lost keyboard focus.
    Focused(bool),
    /// The surface became entirely hidden, or stopped being so.
    ///
    /// A hidden surface must not be redrawn on its own account: an animation behind a minimised
    /// window otherwise runs the whole pipeline every frame for ever. It must still be given the
    /// frames that timers ask for, or work waiting on a timer behind a minimised window never
    /// resumes.
    Occluded(bool),
    /// The desktop's light or dark preference changed.
    ColorSchemeChanged(ColorScheme),
    /// A pointer did something.
    Pointer {
        /// What it did.
        action: PointerAction,
        /// Which pointer, where, and how hard.
        event: PointerEvent,
        /// Which modifiers were held.
        modifiers: Modifiers,
        /// When it happened.
        timestamp: Timestamp,
    },
    /// A wheel or a scroll gesture asked to scroll.
    Wheel {
        /// How far, and where in the gesture.
        event: WheelEvent,
        /// Which modifiers were held.
        modifiers: Modifiers,
        /// When it happened.
        timestamp: Timestamp,
    },
    /// A key went down or came up.
    Key {
        /// Which way it went.
        state: KeyState,
        /// Which key, under three readings.
        event: KeyEvent,
        /// Which modifiers were held.
        modifiers: Modifiers,
        /// When it happened.
        timestamp: Timestamp,
    },
    /// The set of held modifiers changed.
    ///
    /// Delivered every time the set moves, including beside the key event that moved it. It is its
    /// own event because the set also changes with no key event at all: a modifier can move while
    /// the surface does not have the keyboard, and a modifier state recovered from key events alone
    /// is then wrong until the next press.
    ModifiersChanged(Modifiers),
    /// An input method did something to the text being composed.
    Ime(ImeEvent),
    /// Content from outside the application was dragged over or dropped on the surface.
    Drag(DragEvent),
}

impl SurfaceEvent {
    /// Whether this event is input from a person, as opposed to a change in the surface itself.
    ///
    /// Input is dispatched into the document; everything else is handled by the loop.
    pub const fn is_input(&self) -> bool {
        matches!(
            self,
            Self::Pointer { .. }
                | Self::Wheel { .. }
                | Self::Key { .. }
                | Self::ModifiersChanged(_)
                | Self::Ime(_)
                | Self::Drag(_)
        )
    }

    /// Which modifiers were held, for the events that carry them.
    ///
    /// [`SurfaceEvent::to_dispatch`] answers with the event and its payload alone, because a
    /// payload describes what happened and modifiers describe the state it happened in. A
    /// dispatcher needs both, so this and [`SurfaceEvent::timestamp`] are how it takes the second
    /// half from the same value rather than matching the enumeration a second time.
    ///
    /// A change in the held set answers with the new set, which is the whole content of that
    /// event even though it dispatches nothing.
    pub const fn modifiers(&self) -> Option<Modifiers> {
        match self {
            Self::Pointer { modifiers, .. }
            | Self::Wheel { modifiers, .. }
            | Self::Key { modifiers, .. }
            | Self::ModifiersChanged(modifiers) => Some(*modifiers),
            _ => None,
        }
    }

    /// When this event happened, for the events that are stamped.
    pub const fn timestamp(&self) -> Option<Timestamp> {
        match self {
            Self::Pointer { timestamp, .. }
            | Self::Wheel { timestamp, .. }
            | Self::Key { timestamp, .. } => Some(*timestamp),
            _ => None,
        }
    }

    /// This event as something that can be dispatched into a document, when it is one.
    ///
    /// This is where the platform's vocabulary meets the document's, and it is a method rather
    /// than a convention so that the correspondence is written down once and can be checked.
    /// Everything a person did has a document event; everything that happened *to* the surface
    /// does not, and answers with nothing.
    ///
    /// Two of the input events also answer with nothing, and for the same reason in both cases:
    /// they change state without being an occurrence. A change in which modifiers are held is
    /// remembered and attached to the next event; a drag merely moving over the surface is not a
    /// drop and must not be dispatched as one.
    ///
    /// ```
    /// use zgui_geom::{CssPx, Point};
    /// use zgui_platform::SurfaceEvent;
    /// use zgui_vocab::{EventKind, Modifiers, PointerAction, PointerEvent, Timestamp};
    ///
    /// let event = SurfaceEvent::Pointer {
    ///     action: PointerAction::Pressed,
    ///     event: PointerEvent::mouse(Point::new(CssPx(0.0), CssPx(0.0))),
    ///     modifiers: Modifiers::NONE,
    ///     timestamp: Timestamp::ORIGIN,
    /// };
    ///
    /// let (kind, payload) = event.to_dispatch().expect("a press is dispatchable");
    /// assert_eq!(kind, EventKind::PointerDown);
    /// assert!(payload.matches(kind));
    /// ```
    pub fn to_dispatch(&self) -> Option<(EventKind, Payload)> {
        match self {
            Self::Pointer { action, event, .. } => {
                Some((action.event_kind(), Payload::Pointer(*event)))
            }
            Self::Wheel { event, .. } => Some((EventKind::Wheel, Payload::Wheel(*event))),
            Self::Key { state, event, .. } => {
                Some((state.event_kind(), Payload::Key(event.clone())))
            }
            Self::Ime(event) => Some((event.event_kind(), Payload::Ime(event.clone()))),
            Self::Drag(DragEvent::Dropped { paths, position }) => Some((
                EventKind::Drop,
                Payload::Drop(DropEvent {
                    paths: paths.clone(),
                    position: *position,
                }),
            )),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SurfaceEvent;
    use crate::surface::drag::DragEvent;
    use crate::theme::ColorScheme;
    use zgui_geom::{CssPx, DevicePx, Point, Size};
    use zgui_vocab::{
        EventKind, ImeEvent, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey,
        PointerAction, PointerEvent, PointerId, PointerKind, ScrollDelta, ScrollPhase, Timestamp,
        WheelEvent,
    };

    fn pointer() -> SurfaceEvent {
        SurfaceEvent::Pointer {
            action: PointerAction::Moved,
            event: PointerEvent::mouse(Point::new(CssPx(1.0), CssPx(2.0))),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        }
    }

    #[test]
    fn input_is_told_apart_from_everything_else() {
        assert!(pointer().is_input());
        assert!(SurfaceEvent::Drag(DragEvent::Left).is_input());
        assert!(!SurfaceEvent::RedrawRequested.is_input());
        assert!(!SurfaceEvent::Occluded(true).is_input());
        assert!(!SurfaceEvent::ColorSchemeChanged(ColorScheme::Dark).is_input());
    }

    #[test]
    fn stamped_events_report_their_time_and_others_do_not() {
        assert_eq!(pointer().timestamp(), Some(Timestamp::ORIGIN));
        assert_eq!(
            SurfaceEvent::Resized(Size::new(DevicePx(1.0), DevicePx(1.0))).timestamp(),
            None
        );
    }

    #[test]
    fn every_dispatchable_event_can_also_be_asked_what_was_held() {
        // Dispatch answers with the payload alone, so an event that dispatches must be able to
        // supply the rest of a handler's context from the same value.
        let held = SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter)),
            modifiers: Modifiers::CONTROL,
            timestamp: Timestamp::ORIGIN,
        };
        assert!(held.to_dispatch().is_some());
        assert_eq!(held.modifiers(), Some(Modifiers::CONTROL));
        assert_eq!(pointer().modifiers(), Some(Modifiers::NONE));
        assert_eq!(
            SurfaceEvent::ModifiersChanged(Modifiers::SHIFT).modifiers(),
            Some(Modifiers::SHIFT)
        );
        assert_eq!(SurfaceEvent::RedrawRequested.modifiers(), None);
    }

    #[test]
    fn every_dispatchable_event_agrees_with_the_payload_it_produces() {
        let dispatchable = [
            pointer(),
            SurfaceEvent::Wheel {
                event: WheelEvent {
                    delta: ScrollDelta::Lines { x: 0.0, y: -1.0 },
                    phase: ScrollPhase::Discrete,
                    position: Point::new(CssPx(0.0), CssPx(0.0)),
                    id: PointerId::MOUSE,
                    kind: PointerKind::Mouse,
                },
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            },
            SurfaceEvent::Key {
                state: KeyState::Pressed,
                event: KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter)),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            },
            SurfaceEvent::Ime(ImeEvent::Commit("x".into())),
            SurfaceEvent::Drag(DragEvent::Dropped {
                paths: vec![std::path::PathBuf::from("/a")],
                position: Point::new(CssPx(1.0), CssPx(1.0)),
            }),
        ];
        for event in dispatchable {
            let (kind, payload) = event
                .to_dispatch()
                .unwrap_or_else(|| panic!("{event:?} should be dispatchable"));
            assert!(
                payload.matches(kind),
                "{event:?} produced a mismatched payload"
            );
        }
    }

    #[test]
    fn a_press_and_a_release_dispatch_as_different_events() {
        let pressed = SurfaceEvent::Pointer {
            action: PointerAction::Pressed,
            event: PointerEvent::mouse(Point::new(CssPx(0.0), CssPx(0.0))),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        };
        let released = SurfaceEvent::Pointer {
            action: PointerAction::Released,
            event: PointerEvent::mouse(Point::new(CssPx(0.0), CssPx(0.0))),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        };
        assert_eq!(
            pressed.to_dispatch().map(|(kind, _)| kind),
            Some(EventKind::PointerDown)
        );
        assert_eq!(
            released.to_dispatch().map(|(kind, _)| kind),
            Some(EventKind::PointerUp)
        );
    }

    #[test]
    fn state_changes_and_a_drag_in_flight_dispatch_nothing() {
        assert!(
            SurfaceEvent::ModifiersChanged(Modifiers::SHIFT)
                .to_dispatch()
                .is_none()
        );
        assert!(
            SurfaceEvent::Drag(DragEvent::Moved {
                position: Point::new(CssPx(0.0), CssPx(0.0))
            })
            .to_dispatch()
            .is_none()
        );
        assert!(SurfaceEvent::Drag(DragEvent::Left).to_dispatch().is_none());
        assert!(SurfaceEvent::RedrawRequested.to_dispatch().is_none());
        assert!(SurfaceEvent::Occluded(true).to_dispatch().is_none());
    }

    #[test]
    fn a_scale_change_carries_the_size_that_came_with_it() {
        let event = SurfaceEvent::ScaleFactorChanged {
            scale_factor: 2.0,
            size: Size::new(DevicePx(1600.0), DevicePx(1200.0)),
        };
        match event {
            SurfaceEvent::ScaleFactorChanged { scale_factor, size } => {
                assert_eq!(scale_factor, 2.0);
                assert_eq!(size.width, DevicePx(1600.0));
            }
            _ => unreachable!("the event was built as a scale change"),
        }
    }
}
