//! Lifecycle edges, lowered into the payloads a listener actually receives.
//!
//! The end of an animation is the only reliable moment at which content that animated *out* may be
//! removed from the tree. A component that guesses the duration in code instead has two durations —
//! one in the stylesheet and one in the code — and they drift the first time a designer changes the
//! stylesheet. Every edge below is a state change the tick already performed, so reporting it costs
//! the push and nothing else.

use zgui_dom::NodeKey;
use zgui_interned::Ident;
use zgui_style::{AnimationEdge, Lifecycle, TimedKind};
use zgui_vocab::{AnimationEvent, AnimationPhase, Payload, TransitionEvent, TransitionPhase};

/// One lifecycle edge, aimed at the element it happened on.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    /// The element the animation is running on, which the event is dispatched at.
    pub node: NodeKey,
    /// What a listener receives.
    pub payload: Payload,
}

impl Edge {
    /// The edge one engine report describes.
    pub fn from_engine(edge: &AnimationEdge) -> Self {
        Self {
            node: edge.node,
            payload: lower(edge),
        }
    }

    /// The kind of event this edge is delivered as.
    pub fn kind(&self) -> zgui_vocab::EventKind {
        match &self.payload {
            Payload::Animation(event) => event.phase.event_kind(),
            Payload::Transition(event) => event.phase.event_kind(),
            // Nothing else is constructed here, and a caller that reaches this has been handed a
            // payload this module did not make.
            _ => zgui_vocab::EventKind::AnimationEnd,
        }
    }
}

/// The payload one engine report is delivered as.
pub fn lower(edge: &AnimationEdge) -> Payload {
    match edge.kind {
        TimedKind::Animation => Payload::Animation(AnimationEvent {
            name: Ident::new(&edge.name),
            elapsed: edge.elapsed,
            phase: match edge.lifecycle {
                Lifecycle::Started => AnimationPhase::Started,
                Lifecycle::Iterated => AnimationPhase::Iterated,
                Lifecycle::Ended => AnimationPhase::Ended,
                Lifecycle::Cancelled => AnimationPhase::Cancelled,
            },
        }),
        TimedKind::Transition => Payload::Transition(TransitionEvent {
            property: Ident::new(&edge.name),
            elapsed: edge.elapsed,
            phase: match edge.lifecycle {
                // A transition has no iteration, so the engine never reports one; the arm exists
                // because the two lifecycles share one enum and a silent fallthrough here would
                // turn an unreported edge into a spurious `transitionstart`.
                Lifecycle::Started | Lifecycle::Iterated => TransitionPhase::Started,
                Lifecycle::Ended => TransitionPhase::Ended,
                Lifecycle::Cancelled => TransitionPhase::Cancelled,
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_dom::{Document, NodeKind};
    use zgui_interned::ElementName;
    use zgui_style::{AnimationEdge, Lifecycle, TimedKind};
    use zgui_vocab::{AnimationPhase, EventKind, TransitionPhase};

    use super::Edge;

    /// One element's key, so an edge has somewhere to be aimed at.
    fn a_node() -> zgui_dom::NodeKey {
        let mut document = Document::new();
        let index = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("box"),
        );
        document.store().key_of(index)
    }

    #[test]
    fn an_animation_that_ended_says_so_and_is_final() {
        let edge = Edge::from_engine(&AnimationEdge {
            node: a_node(),
            kind: TimedKind::Animation,
            name: "fade-out".into(),
            lifecycle: Lifecycle::Ended,
            elapsed: Duration::from_millis(150),
        });
        let event = edge.payload.as_animation().expect("an animation payload");
        assert_eq!(event.phase, AnimationPhase::Ended);
        assert_eq!(event.elapsed, Duration::from_millis(150));
        assert!(event.is_final());
        assert_eq!(edge.kind(), EventKind::AnimationEnd);
    }

    #[test]
    fn a_transition_is_named_by_the_property_it_moves() {
        let edge = Edge::from_engine(&AnimationEdge {
            node: a_node(),
            kind: TimedKind::Transition,
            name: "background-color".into(),
            lifecycle: Lifecycle::Cancelled,
            elapsed: Duration::ZERO,
        });
        let event = edge.payload.as_transition().expect("a transition payload");
        assert_eq!(event.property.as_str(), "background-color");
        assert_eq!(event.phase, TransitionPhase::Cancelled);
        assert_eq!(edge.kind(), EventKind::TransitionCancel);
    }
}
