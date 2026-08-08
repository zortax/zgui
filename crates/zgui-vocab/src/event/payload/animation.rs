//! The lifecycle of a running animation or transition.

use core::time::Duration;

use zgui_interned::Ident;

use crate::event::kind::EventKind;

/// Which generated-content pseudo-element a lifecycle event happened on.
///
/// A pseudo-element has no node of its own, so its events are dispatched at the element it was
/// generated from. Without this marker a listener on an element that animates both its own box and
/// its `::before` receives two indistinguishable events, and the one thing it has to decide — which
/// of them means "the exit animation is over, the content may go" — cannot be decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Pseudo {
    /// The box generated before the element's own content.
    Before,
    /// The box generated after it.
    After,
}

impl Pseudo {
    /// The selector this is written as, without the colons.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
        }
    }
}

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
///     pseudo: None,
/// };
/// assert!(event.is_final());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationEvent {
    /// The name the animation was declared under.
    pub name: Ident,
    /// How long the animation had been running, excluding any delay.
    ///
    /// A negative `animation-delay` starts the animation part-way through, and this reports where
    /// it started from rather than zero, which is what makes the number comparable with the
    /// stylesheet's own.
    pub elapsed: Duration,
    /// Which stage of its life this reports.
    pub phase: AnimationPhase,
    /// The pseudo-element the animation runs on, when it is not the element's own box.
    pub pseudo: Option<Pseudo>,
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
    ///
    /// A negative `transition-delay` starts the transition part-way through, and this reports where
    /// it started from rather than zero.
    pub elapsed: Duration,
    /// Which stage of its life this reports.
    pub phase: TransitionPhase,
    /// The pseudo-element the transition runs on, when it is not the element's own box.
    pub pseudo: Option<Pseudo>,
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
            pseudo: None,
        };
        assert!(animation(AnimationPhase::Ended).is_final());
        assert!(animation(AnimationPhase::Cancelled).is_final());
        assert!(!animation(AnimationPhase::Started).is_final());

        let transition = |phase| TransitionEvent {
            property: Ident::new("opacity"),
            elapsed: Duration::ZERO,
            phase,
            pseudo: None,
        };
        assert!(transition(TransitionPhase::Ended).is_final());
        assert!(!transition(TransitionPhase::Running).is_final());
    }
}
