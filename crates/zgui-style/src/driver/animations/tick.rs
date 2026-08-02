//! Advancing every running animation one frame, and reporting what that produced.
//!
//! The tick is mechanical on purpose. It moves each animation through its states, drops the ones
//! the cascade cancelled, samples what the survivors currently evaluate to, and writes down every
//! edge it crossed. It marks nothing, writes into no node and decides no policy: what the caller
//! does with an element that is animating a colour is the caller's, and keeping that out of here is
//! what stops two different answers to the same question existing in two crates.
//!
//! # Why the edges are collected rather than looked for afterwards
//!
//! Every edge a lifecycle event reports is a state change this loop performs. Recovering them
//! afterwards would mean keeping a copy of every animation's previous state and diffing it, which
//! is both the same information and a second place for it to be wrong — and the case that matters,
//! an animation that ends, has already been removed from the table by the time anything could look.

use std::time::Duration;

use style::animation::{Animation, AnimationState, ElementAnimationSet, KeyframesIterationState};
use style::context::SharedStyleContext;
use style::properties::longhands::animation_fill_mode::computed_value::single_value::T as FillMode;
use zgui_dom::side::{AnimOverride, AnimPlacement};
use zgui_dom::{Document, NodeIndex, NodeKey};

use crate::driver::animations::sample::{self, AnimatedProperties};
use crate::driver::animations::set::{self, Animations};

/// Whether an edge belongs to a keyframe animation or to a transition.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TimedKind {
    /// A `@keyframes` animation, which has a name.
    Animation,
    /// A transition, which is named by the property it moves.
    Transition,
}

/// A moment in a running animation's life.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Lifecycle {
    /// The delay is over and the value has begun moving.
    Started,
    /// One iteration finished and another began.
    Iterated,
    /// The value reached its destination.
    Ended,
    /// It was stopped before it got there.
    Cancelled,
}

/// One edge one animation crossed during one tick.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AnimationEdge {
    /// The element the animation is running on.
    pub node: NodeKey,
    /// Whether this is an animation or a transition.
    pub kind: TimedKind,
    /// The `@keyframes` name, or the property a transition moves.
    pub name: String,
    /// Which edge was crossed.
    pub lifecycle: Lifecycle,
    /// How long the animation had been running, excluding its delay.
    pub elapsed: Duration,
}

/// One element that is animating, as of this tick.
#[derive(Clone, Debug)]
pub struct ElementAnimation {
    /// The element, as a name that stays valid past the frame it was taken in.
    pub node: NodeKey,
    /// Its slot, which is what an invalidation mark is written against.
    pub index: NodeIndex,
    /// Which kinds of property its animations are moving.
    pub properties: AnimatedProperties,
    /// What those properties currently evaluate to.
    ///
    /// Sampled for every animating element, because whether the values are usable is decided by the
    /// caller and sampling costs one shallow copy of a cascade result. Empty when the animation
    /// moves nothing this stage can express, which is exactly when the caller sends it back through
    /// the cascade instead.
    pub values: AnimOverride,
    /// Where those animations are putting the box, when they are moving it at all.
    ///
    /// Present only for an element whose animations move a transform it already had. An animation
    /// that gives an element its first transform, or takes its last one away, changes what the box
    /// tree and the painting order make of the element as well as where it is drawn — and neither
    /// of those reads anything but the shared style, so that element is reported as cascading and
    /// this is `None`.
    pub placement: Option<AnimPlacement>,
    /// Whether anything on this element is still to be advanced by a later frame.
    ///
    /// False for an element reported only because a finished animation is still *holding* a value:
    /// `animation-fill-mode: forwards` keeps the last keyframe in force for the rest of the
    /// element's life, so those values are still what the element is drawn from, and the loop must
    /// nonetheless be allowed to sleep. A caller that conflated the two either drops the held value
    /// on the frame after the animation ended, or never parks again.
    pub advancing: bool,
    /// Whether one of this element's animations crossed a lifecycle edge on this tick.
    ///
    /// True on the frame an animation starts, iterates or ends, and it is the *ending* one that
    /// matters: an animation is finished by the same tick that reports its end, so by then there is
    /// nothing left to advance — and the final value it settled on has still to be resolved. An
    /// element whose values come from the cascade has that frame and no other in which to ask for
    /// one.
    pub crossed: bool,
}

/// What one tick did.
#[derive(Clone, Debug, Default)]
pub struct AnimationReport {
    /// Every element still animating after the tick.
    pub elements: Vec<ElementAnimation>,
    /// Every lifecycle edge the tick crossed, in no particular order.
    pub edges: Vec<AnimationEdge>,
}

impl AnimationReport {
    /// Whether the tick found nothing running.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty() && self.edges.is_empty()
    }
}

/// Advances every animation in `animations` to `now` and reports what is still running.
///
/// The clock is stored on the way through, so the cascade that runs later in the same frame
/// resolves animated declarations at the very time this sampled them. A frame in which those two
/// disagree draws one value and cascades another, and the difference is one frame of jitter that no
/// assertion about either half can see.
pub(crate) fn advance(
    animations: &mut Animations,
    document: &Document,
    context: &SharedStyleContext<'_>,
    now: f64,
) -> AnimationReport {
    animations.set_now(super::AnimationTime(now));
    let mut report = AnimationReport::default();
    if animations.is_empty() {
        return report;
    }

    // One element may have several rows — its own, and one for each generated-content
    // pseudo-element the cascade started an animation on — and everything below the tick is
    // per element: one override column, one obligation, one count. So the rows are stepped in
    // place and then folded, rather than reported one row at a time.
    let mut folded: Vec<Row> = Vec::new();
    let mut sets = animations.document_set().sets.write();
    sets.retain(|key, set| {
        let index = set::node_of(key);
        if document.store().try_core(index).is_none() {
            // The element is gone. Its animations went with it, and nothing can be reported to a
            // node that no longer exists.
            return false;
        }
        let node = document.store().key_of(index);
        collect_cancelled(set, node, &mut report);
        set.clear_canceled_animations();
        let crossed = step(set, node, now, &mut report);
        let alive = !set.is_empty();

        let advancing = alive && set.running_animation_and_transition_count() > 0;
        let mut properties = sample::properties(set, now);
        if properties.is_empty() {
            // Nothing is being interpolated any more, and the tick that becomes true on is the one
            // frame in which the element's own computed style can be settled. Until an animation
            // ends, that style holds the value the property had when the animation *started*: the
            // cascade that created it evaluated the animation at time zero, and every frame since
            // has come from the interpolation rather than from the cascade. So the moment the
            // interpolation stops, the style underneath it is the value the element began at, and
            // only another cascade replaces that with the one the style sheet asks for now.
            //
            // Reported for **both** paths, because both are drawn from that style once the
            // animation lets go of them. An element the cascade was animating reads it directly; an
            // element that was being repainted from a private override reads it the moment the
            // override is dropped — and a transition left unreported here therefore does not stop
            // at its destination but snaps back to where it set off from, one frame after it
            // arrived. On a `:hover` transition that is a control which lights up for the length of
            // the transition and then goes dark again while the pointer is still on it.
            //
            // Reported as a cascade rather than as a repaint, whatever the animation was moving:
            // there are no values left to compose over anything, and what is owed is exactly one
            // cascade. Reported with nothing advancing, because there is nothing to come back for.
            if !crossed {
                return alive;
            }
            fold(
                &mut folded,
                index,
                AnimatedProperties::CASCADED,
                false,
                true,
            );
            return alive;
        }
        // A pseudo-element has no row in any per-node table, so there is nowhere private to write
        // its interpolated value. Its element goes back through the cascade instead, which is where
        // a generated-content style is produced in the first place.
        if key.pseudo_element.is_some() {
            properties |= AnimatedProperties::CASCADED;
        }
        // Sampled before the row is folded rather than after, because what the sample finds can
        // move the element onto another path: an animation that gives an element its first
        // transform changes what the box tree and the painting order make of it, and the summary
        // of *which* properties moved cannot see the difference between that and a transform being
        // nudged. So the classification is only final once the two styles have been compared.
        let mut sampled = None;
        if properties.is_paint_only() || properties.is_placement_only() {
            let Some(style) = document.node(index).primary_style() else {
                return alive;
            };
            let taken = sample::values(set, context, &style);
            if taken.demoted {
                properties |= AnimatedProperties::CASCADED;
            } else {
                sampled = Some(taken);
            }
        }
        let entry = fold(&mut folded, index, properties, advancing, crossed);
        if let Some(sampled) = sampled {
            if entry.values.is_empty() {
                entry.values = sampled.paint;
            }
            if entry.placement.is_none() {
                entry.placement = sampled.placement;
            }
        }
        alive
    });
    drop(sets);

    report.elements = folded
        .into_iter()
        .map(|row| {
            // A row demoted to the cascade after it was sampled carries values nothing will read,
            // and leaving them on it would be a second answer to which path the element took.
            let usable = row.properties.is_paint_only() || row.properties.is_placement_only();
            ElementAnimation {
                node: document.store().key_of(row.index),
                index: row.index,
                properties: row.properties,
                values: if usable {
                    row.values
                } else {
                    AnimOverride::new()
                },
                placement: if usable { row.placement } else { None },
                advancing: row.advancing,
                crossed: row.crossed,
            }
        })
        .collect();
    report
}

/// One element's tick, before it is turned into an [`ElementAnimation`].
struct Row {
    /// The element.
    index: NodeIndex,
    /// What its animations move.
    properties: AnimatedProperties,
    /// What the paint-only half of them evaluates to.
    values: AnimOverride,
    /// Where the placement half of them puts the box.
    placement: Option<AnimPlacement>,
    /// Whether anything on it is still to be advanced.
    advancing: bool,
    /// Whether anything on it crossed a lifecycle edge.
    crossed: bool,
}

/// Merges one animation set's answer into the element's row, and hands the row back.
///
/// An element may have several sets — its own and one per generated-content pseudo-element — and
/// everything downstream of the tick is per element: one override column, one obligation, one
/// count. So the sets are folded together here rather than reported one at a time.
fn fold(
    folded: &mut Vec<Row>,
    index: NodeIndex,
    properties: AnimatedProperties,
    advancing: bool,
    crossed: bool,
) -> &mut Row {
    let position = match folded.iter().position(|row| row.index == index) {
        Some(position) => {
            let entry = &mut folded[position];
            entry.properties |= properties;
            entry.advancing |= advancing;
            entry.crossed |= crossed;
            position
        }
        None => {
            folded.push(Row {
                index,
                properties,
                values: AnimOverride::new(),
                placement: None,
                advancing,
                crossed,
            });
            folded.len() - 1
        }
    };
    &mut folded[position]
}

/// Records the edges for everything the cascade cancelled since the last tick.
fn collect_cancelled(set: &ElementAnimationSet, node: NodeKey, report: &mut AnimationReport) {
    for animation in &set.animations {
        if animation.state == AnimationState::Canceled {
            report.edges.push(AnimationEdge {
                node,
                kind: TimedKind::Animation,
                name: animation.name.to_string(),
                lifecycle: Lifecycle::Cancelled,
                elapsed: Duration::ZERO,
            });
        }
    }
    for transition in &set.transitions {
        if transition.state == AnimationState::Canceled {
            report.edges.push(AnimationEdge {
                node,
                kind: TimedKind::Transition,
                name: property_name(transition),
                lifecycle: Lifecycle::Cancelled,
                elapsed: Duration::ZERO,
            });
        }
    }
}

/// Moves one element's animations and transitions on by one frame.
///
/// Returns whether any lifecycle edge was crossed, which is what makes the frame an animation ends
/// on the frame its final value is applied on: by then there is nothing left to advance.
fn step(
    set: &mut ElementAnimationSet,
    node: NodeKey,
    now: f64,
    report: &mut AnimationReport,
) -> bool {
    // The transitions that finished on an *earlier* tick, dropped here rather than where they
    // finished. A completed transition is what tells the cascade not to start a second one: the
    // element's computed style holds the value the transition interpolated into it, so the cascade
    // that concludes the transition compares that against the destination, finds them different,
    // and starts a fresh transition over the last fraction of the distance — for ever, a few
    // hundredths at a time — unless the completed one is still there to say the destination has
    // already been reached. So it survives exactly the one tick that cascade runs in.
    set.transitions
        .retain(|transition| transition.state != AnimationState::Finished);
    let before = report.edges.len();
    for animation in &mut set.animations {
        let name = animation.name.to_string();
        if animation.state == AnimationState::Pending && now >= animation.started_at {
            animation.state = AnimationState::Running;
            report.edges.push(AnimationEdge {
                node,
                kind: TimedKind::Animation,
                name: name.clone(),
                lifecycle: Lifecycle::Started,
                elapsed: Duration::ZERO,
            });
        }
        if animation.iterate_if_necessary(now) {
            report.edges.push(AnimationEdge {
                node,
                kind: TimedKind::Animation,
                name: name.clone(),
                lifecycle: Lifecycle::Iterated,
                elapsed: active_duration(animation, iterations_done(animation)),
            });
        }
        if animation.state == AnimationState::Running && animation.has_ended(now) {
            animation.state = AnimationState::Finished;
            report.edges.push(AnimationEdge {
                node,
                kind: TimedKind::Animation,
                name,
                lifecycle: Lifecycle::Ended,
                elapsed: active_duration(animation, iterations_done(animation) + 1.0),
            });
        }
    }
    for transition in &mut set.transitions {
        let name = property_name(transition);
        if transition.state == AnimationState::Pending && now >= transition.start_time {
            transition.state = AnimationState::Running;
            report.edges.push(AnimationEdge {
                node,
                kind: TimedKind::Transition,
                name: name.clone(),
                lifecycle: Lifecycle::Started,
                elapsed: Duration::ZERO,
            });
        }
        if transition.state == AnimationState::Running && transition.has_ended(now) {
            transition.state = AnimationState::Finished;
            report.edges.push(AnimationEdge {
                node,
                kind: TimedKind::Transition,
                name,
                // The transition's own duration, not the wall time since it started. A frame
                // arrives when it arrives, so the two differ by up to a frame, and a listener that
                // compares the number against the one in the stylesheet compares it against that.
                lifecycle: Lifecycle::Ended,
                elapsed: seconds(transition.property_animation.duration),
            });
        }
    }
    // A finished animation is dropped unless its fill mode says its last keyframe stays in force.
    // Leaving a merely-finished one in the table would keep the element reporting values it no
    // longer owns; dropping a filling one is `animation-fill-mode: forwards` snapping back to the
    // base style on the frame after it ended, which is what the property exists to prevent.
    set.animations.retain(|animation| {
        animation.state != AnimationState::Finished || fills_forwards(animation)
    });
    report.edges.len() != before
}

/// Whether a finished animation's last keyframe stays in force.
fn fills_forwards(animation: &Animation) -> bool {
    matches!(animation.fill_mode, FillMode::Forwards | FillMode::Both)
}

/// How many iterations of an animation have run to completion.
fn iterations_done(animation: &Animation) -> f64 {
    match animation.iteration_state {
        KeyframesIterationState::Finite(current, _)
        | KeyframesIterationState::Infinite(current) => current,
    }
}

/// How long an animation has been running after `iterations` of it, excluding its delay.
fn active_duration(animation: &Animation, iterations: f64) -> Duration {
    seconds(iterations * animation.duration)
}

/// A duration in seconds, floored at zero so a negative never panics.
fn seconds(value: f64) -> Duration {
    Duration::from_secs_f64(value.max(0.0))
}

/// The property a transition moves, spelled the way a stylesheet spells it.
fn property_name(transition: &style::animation::Transition) -> String {
    transition
        .property_animation
        .property_id()
        .name()
        .to_string()
}
