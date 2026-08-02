//! Placing a child against an inclusive run, in a bounded number of steps.

use zgui_profile::{Counter, counter};

use crate::arena::store::DocumentStore;
use crate::id::node_key::NodeIndex;

/// How far the scan that places a child against a run walks, in each direction.
///
/// A child inside a run is at most half the run's width from one of its two ends, so every run at
/// most twice this wide is placed exactly; so is every child this close to an end of a wider run,
/// or to an end of the child list. Past that the scan gives up and the record widens to every child
/// of its owner, which costs probes rather than correctness.
pub const SCAN: usize = 64;

/// Where a child sits relative to an inclusive run, as far as a bounded scan could tell.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(super) enum Placement {
    /// Ahead of the run's first child.
    Before,
    /// Between the run's two ends, so the run covers it already.
    Inside,
    /// Past the run's last child.
    After,
    /// Neither end of the run nor either end of the child list was within reach.
    Unplaced,
}

/// Places `child` against the run from `first` to `last`, walking at most [`SCAN`] links each way.
///
/// The scan steps outwards from `child` in both directions at once and stops at the first landmark
/// it meets, which is enough to decide: `first` and `last` cannot both lie on the same side of a
/// child between them, so meeting `last` on the way back means the child is past the run and
/// meeting `first` on the way back means it is inside one, and the forward walk says the mirror of
/// that. Running off either end of the child list decides it too — a run's ends are children of the
/// same list, so passing the head without meeting one puts the child ahead of both.
///
/// Stepping outwards from the child rather than inwards from the run's ends is what makes the cost
/// the child's distance from the nearest landmark instead of the run's width.
///
/// # Panics
///
/// Panics if `child` names no live node of `store`, or if a sibling link leads to one.
pub(super) fn place(
    store: &DocumentStore,
    child: NodeIndex,
    first: NodeIndex,
    last: NodeIndex,
) -> Placement {
    let mut back = store.core(child).prev_sibling();
    let mut ahead = store.core(child).next_sibling();
    let mut steps = 0;
    let mut placement = Placement::Unplaced;
    for _ in 0..SCAN {
        steps += 1;
        match back {
            None => {
                placement = Placement::Before;
                break;
            }
            Some(node) if node == last => {
                placement = Placement::After;
                break;
            }
            Some(node) if node == first => {
                placement = Placement::Inside;
                break;
            }
            Some(node) => back = store.core(node).prev_sibling(),
        }
        steps += 1;
        match ahead {
            None => {
                placement = Placement::After;
                break;
            }
            Some(node) if node == first => {
                placement = Placement::Before;
                break;
            }
            Some(node) if node == last => {
                placement = Placement::Inside;
                break;
            }
            Some(node) => ahead = store.core(node).next_sibling(),
        }
    }
    counter::add(Counter::DirtyChildSteps, steps);
    placement
}
