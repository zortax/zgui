//! Anything that points: a mouse, a finger, a stylus.

use zgui_geom::{Css, CssPx, Point};

use crate::event::kind::EventKind;
use crate::time::Timestamp;

/// Which physical pointer produced an event.
///
/// Several pointers can be down at once — two fingers, or a finger and a stylus — so an
/// interaction is tracked by identifier and not by "the pointer". A mouse keeps one identifier for
/// the life of the process; a finger's identifier lasts from touch-down to lift and is then free
/// to be reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PointerId(u64);

impl PointerId {
    /// The identifier of the system mouse, which is the pointer that always exists.
    pub const MOUSE: Self = Self(0);

    /// The identifier with the given raw value.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw value.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// What kind of device a pointer is.
///
/// Touch is a pointer kind rather than a separate event stream. A control written against pointer
/// events therefore works under a finger without being written twice, and the places where the
/// difference genuinely matters — hovering is impossible with a finger, a stylus reports pressure
/// — read this instead of subscribing to a different event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PointerKind {
    /// A mouse or anything that behaves like one, including most trackpads.
    #[default]
    Mouse,
    /// A finger on a touch surface.
    Touch,
    /// A pen or stylus.
    Pen,
    /// A device the platform did not identify.
    Unknown,
}

impl PointerKind {
    /// Whether this kind of pointer can rest over an element without pressing it.
    ///
    /// A finger cannot, which is why a control whose only affordance appears on hover is
    /// unreachable by touch.
    pub const fn can_hover(self) -> bool {
        matches!(self, Self::Mouse | Self::Pen)
    }
}

/// Which button of a pointer was pressed or released.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PointerButton {
    /// The primary button: the left mouse button by default, a touch contact, a pen tip.
    #[default]
    Primary,
    /// The secondary button, which conventionally opens a context menu.
    Secondary,
    /// The middle button, usually the scroll wheel pressed.
    Middle,
    /// The button that navigates back.
    Back,
    /// The button that navigates forward.
    Forward,
    /// A button beyond the named ones, by its platform index.
    Other(u16),
}

/// What a pointer just did.
///
/// This is the discriminant behind the pointer event kinds, kept as a value so a backend can
/// report what happened without knowing the names events are registered under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PointerAction {
    /// The pointer moved onto the element.
    Entered,
    /// The pointer moved while over the element.
    Moved,
    /// A button went down.
    Pressed,
    /// A button came up.
    Released,
    /// The pointer moved off the element.
    Left,
    /// The interaction was taken over by something else and will produce no release.
    ///
    /// A gesture recogniser claiming a drag, or a window losing its input, both end an
    /// interaction without a release, and a control that only listens for releases stays stuck
    /// down forever.
    Cancelled,
}

impl PointerAction {
    /// The kind of event this action is delivered as.
    pub const fn event_kind(self) -> EventKind {
        match self {
            Self::Entered => EventKind::PointerEnter,
            Self::Moved => EventKind::PointerMove,
            Self::Pressed => EventKind::PointerDown,
            Self::Released => EventKind::PointerUp,
            Self::Left => EventKind::PointerLeave,
            Self::Cancelled => EventKind::PointerCancel,
        }
    }
}

/// What a pointer event carries.
///
/// Position is in CSS pixels relative to the window's top-left corner, already divided by the
/// output's scale factor, so a control does no arithmetic to compare it with its own bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    /// Which pointer this is.
    pub id: PointerId,
    /// What kind of device it is.
    pub kind: PointerKind,
    /// Whether this is the pointer that drives compatibility behaviour.
    ///
    /// Exactly one pointer of an interaction is primary — the mouse, or the first finger down.
    /// A control that has no multi-pointer behaviour of its own ignores everything else and
    /// therefore does something sensible under a second finger instead of something wrong.
    pub primary: bool,
    /// Where the pointer is, in CSS pixels from the window's top-left corner.
    pub position: Point<CssPx, Css>,
    /// Which button changed, on a press or a release.
    pub button: Option<PointerButton>,
    /// How hard the pointer is pressed, from zero to one, when the device reports it.
    pub pressure: Option<f32>,
}

impl PointerEvent {
    /// A mouse event at `position` with nothing else set.
    pub fn mouse(position: Point<CssPx, Css>) -> Self {
        Self {
            id: PointerId::MOUSE,
            kind: PointerKind::Mouse,
            primary: true,
            position,
            button: None,
            pressure: None,
        }
    }

    /// The same event with `button` recorded as the one that changed.
    pub fn with_button(mut self, button: PointerButton) -> Self {
        self.button = Some(button);
        self
    }

    /// What a move of this pointer keeps when a later move takes its place in a queue.
    pub fn sample(&self, timestamp: Timestamp) -> PointerSample {
        PointerSample {
            position: self.position,
            pressure: self.pressure,
            timestamp,
        }
    }
}

/// One pointer position a queue folded into the move delivered after it.
///
/// Moves that arrive between two frames are delivered as one, so a frame routes and settles one
/// event per pointer. What the folded moves carried is kept here, in the order they arrived, for
/// the one consumer that wants every sample: a stroke drawn by hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerSample {
    /// Where the pointer was, in CSS pixels from the window's top-left corner.
    pub position: Point<CssPx, Css>,
    /// How hard it was pressed, when the device reports it.
    pub pressure: Option<f32>,
    /// When the sample was taken.
    pub timestamp: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::{PointerAction, PointerButton, PointerEvent, PointerId, PointerKind};
    use crate::event::kind::EventKind;
    use zgui_geom::{Css, CssPx, Point};

    #[test]
    fn every_action_maps_to_its_own_event_kind() {
        let actions = [
            PointerAction::Entered,
            PointerAction::Moved,
            PointerAction::Pressed,
            PointerAction::Released,
            PointerAction::Left,
            PointerAction::Cancelled,
        ];
        let kinds: Vec<EventKind> = actions.iter().map(|a| a.event_kind()).collect();
        for (index, kind) in kinds.iter().enumerate() {
            assert!(
                !kinds[index + 1..].contains(kind),
                "{:?} shares an event kind with a later action",
                actions[index]
            );
        }
    }

    /// Fails to compile when a variant is added, naming everywhere that has to be extended.
    ///
    /// `PointerAction` is `#[non_exhaustive]`, so no consumer crate can write a match that a new
    /// variant breaks — every list of actions outside this crate is a hand-written one that stays
    /// green when it falls behind. This is the only place the compiler can be made to object, so
    /// it is the place that carries the list.
    #[test]
    fn every_action_is_covered_here() {
        fn covered(action: PointerAction) -> &'static str {
            match action {
                // Adding a variant breaks this match. When it does, extend:
                //   - `PointerAction::event_kind` above, with the kind it dispatches as;
                //   - `every_action_maps_to_its_own_event_kind`, which proves the kinds are
                //     distinct;
                //   - `zgui_input::normalize`'s `every_pointer_action_becomes_its_own_event`,
                //     which proves the action survives normalisation into a dispatchable event.
                PointerAction::Entered => "entered",
                PointerAction::Moved => "moved",
                PointerAction::Pressed => "pressed",
                PointerAction::Released => "released",
                PointerAction::Left => "left",
                PointerAction::Cancelled => "cancelled",
            }
        }
        assert_eq!(covered(PointerAction::Entered), "entered");
        assert_eq!(covered(PointerAction::Left), "left");
    }

    #[test]
    fn only_a_mouse_or_a_pen_can_hover() {
        assert!(PointerKind::Mouse.can_hover());
        assert!(PointerKind::Pen.can_hover());
        assert!(!PointerKind::Touch.can_hover());
        assert!(!PointerKind::Unknown.can_hover());
    }

    #[test]
    fn a_mouse_event_is_primary_and_carries_the_mouse_identifier() {
        let event = PointerEvent::mouse(Point::new(CssPx(4.0), CssPx(8.0)))
            .with_button(PointerButton::Secondary);
        assert_eq!(event.id, PointerId::MOUSE);
        assert!(event.primary);
        assert_eq!(event.button, Some(PointerButton::Secondary));
        assert_eq!(
            event.position,
            Point::<CssPx, Css>::new(CssPx(4.0), CssPx(8.0))
        );
    }
}
