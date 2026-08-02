//! The lifecycle of a running animation or transition.

use core::time::Duration;

use zgui_interned::Ident;

use crate::event::kind::EventKind;

/// A stage in a named animation's life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AnimationPhase {
    /// The animation began, after any delay it had.
    Started,
    /// One iteration finished and another began.
    Iterated,
    /// The animation finished on its own.
    Ended,
    /// The animation was stopped before it finished.
    Cancelled,
}

impl AnimationPhase {
    /// The kind of event this phase is delivered as.
    pub const fn event_kind(self) -> EventKind {
        match self {
            Self::Started => EventKind::AnimationStart,
            Self::Iterated => EventKind::AnimationIteration,
            Self::Ended => EventKind::AnimationEnd,
            Self::Cancelled => EventKind::AnimationCancel,
        }
    }
}

/// What an animation lifecycle event carries.
///
/// The end of an animation is the only reliable moment at which content that animated *out* may
/// be removed. Without it, a component that fades something away must either guess the duration
/// in code — where it will drift from the duration in the stylesheet — or leave the content in the
/// tree forever.
///
/// ```
/// use core::time::Duration;
/// use zgui_interned::Ident;
/// use zgui_vocab::{AnimationEvent, AnimationPhase};
///
/// let event = AnimationEvent {
///     name: Ident::new("fade-out"),
///     elapsed: Duration::from_millis(150),
///     phase: AnimationPhase::Ended,
/// };
/// assert!(event.is_final());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationEvent {
    /// The name the animation was declared under.
    pub name: Ident,
    /// How long the animation had been running, excluding any delay.
    pub elapsed: Duration,
    /// Which stage of its life this reports.
    pub phase: AnimationPhase,
}

impl AnimationEvent {
    /// Whether this is the last event the animation will produce.
    pub const fn is_final(&self) -> bool {
        matches!(
            self.phase,
            AnimationPhase::Ended | AnimationPhase::Cancelled
        )
    }
}

/// A stage in a transition's life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TransitionPhase {
    /// The transition was created and is waiting out its delay.
    Running,
    /// The delay is over and the value has begun moving.
    Started,
    /// The value reached its destination.
    Ended,
    /// The transition was stopped before the value arrived.
    Cancelled,
}

impl TransitionPhase {
    /// The kind of event this phase is delivered as.
    pub const fn event_kind(self) -> EventKind {
        match self {
            Self::Running => EventKind::TransitionRun,
            Self::Started => EventKind::TransitionStart,
            Self::Ended => EventKind::TransitionEnd,
            Self::Cancelled => EventKind::TransitionCancel,
        }
    }
}

/// What a transition lifecycle event carries.
///
/// A transition is named by the property it animates rather than by a declared name, which is the
/// only difference from an animation that matters to a handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionEvent {
    /// The property whose value is moving.
    pub property: Ident,
    /// How long the transition had been running, excluding its delay.
    pub elapsed: Duration,
    /// Which stage of its life this reports.
    pub phase: TransitionPhase,
}

impl TransitionEvent {
    /// Whether this is the last event the transition will produce.
    pub const fn is_final(&self) -> bool {
        matches!(
            self.phase,
            TransitionPhase::Ended | TransitionPhase::Cancelled
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{AnimationEvent, AnimationPhase, TransitionEvent, TransitionPhase};
    use core::time::Duration;
    use zgui_interned::Ident;

    #[test]
    fn every_animation_phase_has_its_own_event_kind() {
        let kinds: Vec<_> = [
            AnimationPhase::Started,
            AnimationPhase::Iterated,
            AnimationPhase::Ended,
            AnimationPhase::Cancelled,
        ]
        .iter()
        .map(|phase| phase.event_kind())
        .collect();
        for (index, kind) in kinds.iter().enumerate() {
            assert!(!kinds[index + 1..].contains(kind));
        }
    }

    #[test]
    fn every_transition_phase_has_its_own_event_kind() {
        let kinds: Vec<_> = [
            TransitionPhase::Running,
            TransitionPhase::Started,
            TransitionPhase::Ended,
            TransitionPhase::Cancelled,
        ]
        .iter()
        .map(|phase| phase.event_kind())
        .collect();
        for (index, kind) in kinds.iter().enumerate() {
            assert!(!kinds[index + 1..].contains(kind));
        }
    }

    #[test]
    fn both_terminal_phases_are_final_and_the_others_are_not() {
        let animation = |phase| AnimationEvent {
            name: Ident::new("fade"),
            elapsed: Duration::ZERO,
            phase,
        };
        assert!(animation(AnimationPhase::Ended).is_final());
        assert!(animation(AnimationPhase::Cancelled).is_final());
        assert!(!animation(AnimationPhase::Started).is_final());

        let transition = |phase| TransitionEvent {
            property: Ident::new("opacity"),
            elapsed: Duration::ZERO,
            phase,
        };
        assert!(transition(TransitionPhase::Ended).is_final());
        assert!(!transition(TransitionPhase::Running).is_final());
    }
}
