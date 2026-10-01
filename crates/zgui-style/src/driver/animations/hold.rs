//! Keeping a finished animation that fills forwards through a restyle of its element.
//!
//! The engine removes every finished animation when it restyles an element, and it ignores the
//! fill mode when it does so. The tick keeps a finished animation that fills forwards, because its
//! last keyframe stays in force. After the engine removes it, the next cascade of the element has
//! no animation rule, and the held values go back to the base style. A dialog that is centred by
//! the last keyframe of its entrance then moves when something inside it is clicked.
//!
//! [`hold`] shows such an animation to the engine as running for the length of one restyle.
//! [`release`] marks it as finished again. The engine still updates its keyframes from the new
//! style, so a held value that reads a changed variable gets the new value.

use smallvec::SmallVec;
use style::Atom;
use style::animation::{AnimationSetKey, AnimationState};
use style::context::SharedStyleContext;
use style::dom::{TElement, TNode};
use style::selector_parser::PseudoElement;

use crate::driver::animations::tick::fills_forwards;

/// The animations [`hold`] marked as running, by row and name.
pub(crate) type Held = SmallVec<[(AnimationSetKey, Atom); 2]>;

/// Marks the finished animations of `element` that fill forwards as running.
///
/// Call it just before the restyle, and give the result to [`release`] just after it.
pub(crate) fn hold<E: TElement>(element: E, shared: &SharedStyleContext) -> Held {
    let mut held = Held::new();
    if shared.animations.sets.read().is_empty() {
        return held;
    }
    let mut sets = shared.animations.sets.write();
    let node = element.as_node().opaque();
    // The engine restyles the element and its two generated boxes in one call.
    for pseudo in [
        None,
        Some(PseudoElement::Before),
        Some(PseudoElement::After),
    ] {
        let key = AnimationSetKey::new(node, pseudo);
        let Some(set) = sets.get_mut(&key) else {
            continue;
        };
        for animation in &mut set.animations {
            if animation.state == AnimationState::Finished && fills_forwards(animation) {
                animation.state = AnimationState::Running;
                held.push((key.clone(), animation.name.clone()));
            }
        }
    }
    held
}

/// Marks the animations that [`hold`] marked as running as finished again.
///
/// An animation that the new style no longer names is removed without a cancel event, because it
/// ended before the restyle and its end was already reported.
pub(crate) fn release(shared: &SharedStyleContext, held: Held) {
    if held.is_empty() {
        return;
    }
    let now = shared.current_time_for_animations;
    let mut sets = shared.animations.sets.write();
    for (key, name) in held {
        let Some(set) = sets.get_mut(&key) else {
            continue;
        };
        set.animations.retain(|animation| {
            animation.name != name || animation.state != AnimationState::Canceled
        });
        for animation in &mut set.animations {
            if animation.name == name
                && animation.state == AnimationState::Running
                && animation.has_ended(now)
            {
                animation.state = AnimationState::Finished;
            }
        }
        if set.is_empty() {
            sets.remove(&key);
        }
    }
}
