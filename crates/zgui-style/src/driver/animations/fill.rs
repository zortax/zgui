//! Finished animations that fill forwards, kept in the table across a traversal.
//!
//! The engine's cascade drops every finished keyframe animation from the table when it styles the
//! element, whatever its fill mode. The table is also where a later match reads the animation
//! declarations from, so an element that fills forwards loses its last keyframe on the first match
//! after the animation ended. A theme change matches every element again, and a surface centred
//! by a filled transform jumps to where its base style puts it.
//!
//! So a traversal sees each such animation as running. Its start time stays where it was, so it
//! evaluates at its end and fills exactly as a finished one does. After the traversal, each one
//! that has still ended is finished again, and the tick reports no second end for it.

use style::Atom;
use style::animation::{AnimationSetKey, AnimationState, DocumentAnimationSet};

use crate::driver::animations::tick::fills_forwards;

/// The animations [`hold`] turned back to running, by element row.
#[must_use = "the held animations stay running until they are released"]
pub(crate) struct Held {
    /// Each row, with the names of the animations held on it.
    rows: Vec<(AnimationSetKey, Vec<Atom>)>,
}

/// Turns every finished animation that fills forwards back to running for one traversal.
pub(crate) fn hold(table: &DocumentAnimationSet) -> Held {
    let mut rows = Vec::new();
    for (key, set) in table.sets.write().iter_mut() {
        let names: Vec<Atom> = set
            .animations
            .iter_mut()
            .filter(|animation| {
                animation.state == AnimationState::Finished && fills_forwards(animation)
            })
            .map(|animation| {
                animation.state = AnimationState::Running;
                animation.name.clone()
            })
            .collect();
        if !names.is_empty() {
            rows.push((key.clone(), names));
        }
    }
    Held { rows }
}

impl Held {
    /// Finishes again every held animation that is still running or that the traversal paused,
    /// and that has ended at `now`. A paused one stays finished, so a later resume reports no
    /// second end.
    ///
    /// An animation the traversal cancelled goes from the table without a cancel, since it stood
    /// after its end. One a new style moved away from its end keeps the state the traversal left it
    /// in, and the next tick reports what happens to it.
    pub(crate) fn release(self, table: &DocumentAnimationSet, now: f64) {
        if self.rows.is_empty() {
            return;
        }
        let mut sets = table.sets.write();
        for (key, names) in self.rows {
            let Some(set) = sets.get_mut(&key) else {
                continue;
            };
            // A held animation had finished, so it stood after its end. One the traversal cancelled
            // goes without a cancel: an animation after its end leaves no event behind.
            set.animations.retain(|animation| {
                !(animation.state == AnimationState::Canceled && names.contains(&animation.name))
            });
            for animation in &mut set.animations {
                let held = matches!(
                    animation.state,
                    AnimationState::Running | AnimationState::Paused(_)
                );
                if held && names.contains(&animation.name) && animation.has_ended(now) {
                    animation.state = AnimationState::Finished;
                }
            }
        }
    }
}
