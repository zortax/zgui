//! How long the values of an element's stepped animations hold still.
//!
//! A keyframe animation that runs on `steps()` changes its values only at its step boundaries, and
//! holds them in between. A tick at every refresh between two boundaries samples the same values
//! again and draws nothing new. This module finds the next boundary, so the caller can sleep until
//! then.
//!
//! The answer is conservative. Every keyframe pair counts as a segment, because a property that one
//! keyframe leaves out is interpolated between the keyframes on either side of it. A boundary where
//! nothing changes costs one tick and no more.

use style::animation::{AnimationState, ElementAnimationSet};
use style::context::SharedStyleContext;
use style::properties::ComputedValues;
use style::properties::longhands::animation_direction::computed_value::single_value::T as Direction;
use style::values::generics::easing::TimingFunction;
use zgui_dom::{Document, Node, NodeIndex};

use crate::driver::animations::set::{self, Animations};

/// The most steps one segment may have for the search to run. A larger count is close enough to a
/// continuous motion that the element ticks at every refresh.
const MAX_STEPS: i32 = 256;

/// The most keyframes an animation may have for the search to run.
const MAX_KEYFRAMES: usize = 16;

/// A progress closer than this to a boundary counts as past it.
const EPSILON: f64 = 1e-9;

/// The next moment, on the animation clock, at which an animation in `set` can change what it
/// evaluates to. `None` when something in `set` moves at every refresh: a transition, an animation
/// with a continuous timing function, or one whose keyframes declare their own.
pub(crate) fn until(
    set: &ElementAnimationSet,
    context: &SharedStyleContext<'_>,
    element: Node<'_>,
    style: &ComputedValues,
    now: f64,
) -> Option<f64> {
    if set
        .transitions
        .iter()
        .any(|it| matches!(it.state, AnimationState::Running | AnimationState::Pending))
    {
        return None;
    }
    let ui = style.get_ui();
    let mut earliest: Option<f64> = None;
    for animation in &set.animations {
        match animation.state {
            AnimationState::Running => {}
            AnimationState::Pending => return None,
            AnimationState::Paused(_) | AnimationState::Finished | AnimationState::Canceled => {
                continue;
            }
        }
        if animation.duration <= 0.0 {
            return None;
        }
        let index = ui
            .animation_name_iter()
            .position(|name| name.as_atom() == Some(&animation.name))?;
        let TimingFunction::Steps(count, _) = ui.animation_timing_function_mod(index) else {
            return None;
        };
        let keyframes = context.stylist.lookup_keyframes(&animation.name, element)?;
        if keyframes
            .steps
            .iter()
            .any(|step| step.declared_timing_function)
        {
            return None;
        }
        let mut offsets: Vec<f64> = keyframes
            .steps
            .iter()
            .map(|step| f64::from(step.start_offset.percentage.0))
            .chain([0.0, 1.0])
            .collect();
        offsets.sort_by(f64::total_cmp);
        offsets.dedup();
        let progress = (now - animation.started_at) / animation.duration;
        let reverse = animation.current_direction == Direction::Reverse;
        let end = animation.current_iteration_end_progress();
        let next = next_boundary(&offsets, count, reverse, progress, end)?;
        let at = animation.started_at + next * animation.duration;
        earliest = Some(earliest.map_or(at, |it| it.min(at)));
    }
    earliest
}

/// The elements of `held` whose animations no longer hold still until the moment recorded for
/// them. `held` pairs each element with that moment, and is sorted by element.
///
/// The cascade runs after the tick, and it can end or replace an animation the tick found
/// holding. Such an element owes a tick on the next frame: the tick drops the value the element
/// was drawn with, or samples the animation that replaced it.
pub(crate) fn lapsed(
    animations: &Animations,
    document: &Document,
    context: &SharedStyleContext<'_>,
    held: &[(NodeIndex, f64)],
    now: f64,
) -> Vec<NodeIndex> {
    // The earliest boundary of each element's sets, or an error once one of them moves at every
    // refresh.
    let mut found: Vec<Result<Option<f64>, ()>> = vec![Ok(None); held.len()];
    let sets = animations.document_set().sets.read();
    for (key, animated) in sets.iter() {
        let index = set::node_of(key);
        let Ok(position) = held.binary_search_by_key(&index, |(element, _)| *element) else {
            continue;
        };
        if animated.running_animation_and_transition_count() == 0 {
            continue;
        }
        let until = if key.pseudo_element.is_some() || document.store().try_core(index).is_none() {
            None
        } else {
            let node = document.node(index);
            node.primary_style()
                .and_then(|style| until(animated, context, node, &style, now))
        };
        found[position] = match (found[position], until) {
            (Ok(was), Some(at)) => Ok(Some(was.map_or(at, |was| was.min(at)))),
            _ => Err(()),
        };
    }
    held.iter()
        .zip(found)
        .filter(|((index, at), found)| {
            *found != Ok(Some(*at)) && document.store().try_core(*index).is_some()
        })
        .map(|((index, _), _)| *index)
        .collect()
}

/// The first progress after `progress` at which an animation over the keyframe `offsets`, with
/// `count` steps in each segment, can change its value. `reverse` runs the keyframes backwards, and
/// `end` is the progress the current iteration ends at, which is a boundary too. `None` when the
/// search is too large to run.
#[must_use]
pub fn next_boundary(
    offsets: &[f64],
    count: i32,
    reverse: bool,
    progress: f64,
    end: f64,
) -> Option<f64> {
    if !(1..=MAX_STEPS).contains(&count) || offsets.len() > MAX_KEYFRAMES {
        return None;
    }
    let steps = f64::from(count);
    let mut next = end;
    let mut offer = |boundary: f64| {
        let at = if reverse { 1.0 - boundary } else { boundary };
        if at > progress + EPSILON && at < next {
            next = at;
        }
    };
    for (first, from) in offsets.iter().enumerate() {
        for to in &offsets[first + 1..] {
            for step in 0..=count {
                offer(from + (to - from) * f64::from(step) / steps);
            }
        }
    }
    (next > progress + EPSILON).then_some(next)
}

#[cfg(test)]
mod tests {
    use super::next_boundary;

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-6)
    }

    #[test]
    fn two_steps_over_a_middle_keyframe_change_every_quarter() {
        let offsets = [0.0, 0.5, 1.0];
        assert!(close(next_boundary(&offsets, 2, false, 0.0, 1.0), 0.25));
        assert!(close(next_boundary(&offsets, 2, false, 0.3, 1.0), 0.5));
        assert!(close(next_boundary(&offsets, 2, false, 0.9, 1.0), 1.0));
    }

    #[test]
    fn a_reverse_run_counts_from_the_end() {
        let offsets = [0.0, 0.2, 1.0];
        assert!(close(next_boundary(&offsets, 1, true, 0.0, 1.0), 0.8));
        assert!(close(next_boundary(&offsets, 1, true, 0.85, 1.0), 1.0));
    }

    #[test]
    fn a_short_last_iteration_ends_at_its_end() {
        assert!(close(next_boundary(&[0.0, 1.0], 4, false, 0.3, 0.4), 0.4));
    }

    #[test]
    fn the_delay_ends_at_the_start() {
        assert!(close(next_boundary(&[0.0, 1.0], 2, false, -0.5, 1.0), 0.0));
    }

    #[test]
    fn a_large_count_ticks_every_refresh() {
        assert_eq!(next_boundary(&[0.0, 1.0], 10_000, false, 0.0, 1.0), None);
        assert_eq!(next_boundary(&[0.0, 1.0], 0, false, 0.0, 1.0), None);
    }
}
