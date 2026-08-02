//! Sizing an atomic inline, and the memo without which it is ruinous.
//!
//! An `inline-block` is a leaf to the line it sits in and a container to everything inside it, so
//! sizing one means running a whole nested layout. The layout algorithms have no shrink-to-fit
//! sizing mode, so each measurement costs three of those nested runs — a minimum-content probe, a
//! maximum-content probe, and a definite pass — and a flex container probes each of its items four
//! times. Twelve nested layouts per atomic inline per layout pass is the unmemoised cost, and it
//! compounds with every width the container is tried at.
//!
//! So the answer is memoised on the constraint that produced it. This is not an optimisation that
//! can be dropped: without it a page with a handful of atomic inlines re-lays out its whole
//! contents on every frame that changes any width.

use rustc_hash::FxHashMap;
use taffy::{AvailableSpace, LayoutInput, LayoutPartialTree, RunMode, Size, SizingMode};
use zgui_dom::side::BoxKey;

use crate::key::to_node_id;
use crate::measure::{MeasureContent, Measured};
use crate::tree::LayoutTree;

/// The constraint one shrink-to-fit answer was computed under.
///
/// Available space is compared by its bits rather than by its value, because two different
/// constraints must never be treated as one and a floating-point comparison of an infinity or a
/// keyword has no useful meaning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Constraint {
    /// The width already fixed, if one was.
    known_width: Option<u32>,
    /// The height already fixed, if one was.
    known_height: Option<u32>,
    /// The space available on the inline axis.
    available_width: SpaceKey,
    /// The space available on the block axis.
    available_height: SpaceKey,
}

impl Constraint {
    /// The constraint a measurement was taken under.
    pub fn new(known: Size<Option<f32>>, available: Size<AvailableSpace>) -> Self {
        Self {
            known_width: known.width.map(f32::to_bits),
            known_height: known.height.map(f32::to_bits),
            available_width: SpaceKey::new(available.width),
            available_height: SpaceKey::new(available.height),
        }
    }
}

/// One axis's available space, as something that can be compared exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SpaceKey {
    /// A definite number of device pixels, by its bits.
    Definite(u32),
    /// As narrow as the content can be.
    MinContent,
    /// As wide as it would like.
    MaxContent,
}

impl SpaceKey {
    /// The key for one axis's available space.
    fn new(space: AvailableSpace) -> Self {
        match space {
            AvailableSpace::Definite(value) => Self::Definite(value.to_bits()),
            AvailableSpace::MinContent => Self::MinContent,
            AvailableSpace::MaxContent => Self::MaxContent,
        }
    }
}

/// Shrink-to-fit answers, held for one layout pass.
#[derive(Debug, Default)]
pub struct AtomicMemo {
    /// What each box measured under each constraint.
    entries: FxHashMap<(BoxKey, Constraint), Measured>,
    /// How many nested layouts were avoided.
    hits: u32,
    /// How many were performed.
    misses: u32,
}

impl AtomicMemo {
    /// A held answer, if there is one.
    pub fn get(&mut self, key: BoxKey, constraint: Constraint) -> Option<Measured> {
        let held = self.entries.get(&(key, constraint)).copied();
        if held.is_some() {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        held
    }

    /// Holds one answer.
    pub fn insert(&mut self, key: BoxKey, constraint: Constraint, measured: Measured) {
        self.entries.insert((key, constraint), measured);
    }

    /// How many nested layouts were avoided.
    pub fn hits(&self) -> u32 {
        self.hits
    }

    /// How many were performed.
    pub fn misses(&self) -> u32 {
        self.misses
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.hits = 0;
        self.misses = 0;
    }
}

/// Measures an atomic inline by running a nested layout of its own subtree.
pub(crate) fn measure<C: MeasureContent>(
    tree: &mut LayoutTree<'_, C>,
    key: BoxKey,
    known: Size<Option<f32>>,
    available: Size<AvailableSpace>,
) -> Measured {
    let constraint = Constraint::new(known, available);
    if let Some(held) = tree.atomic_memo_mut().get(key, constraint) {
        return held;
    }
    let output = tree.compute_child_layout(
        to_node_id(key),
        LayoutInput {
            run_mode: RunMode::PerformLayout,
            sizing_mode: SizingMode::InherentSize,
            axis: taffy::RequestedAxis::Both,
            known_dimensions: known,
            parent_size: available.into_options(),
            available_space: available,
            vertical_margins_are_collapsible: taffy::Line::FALSE,
        },
    );
    let last = tree
        .store()
        .state(key)
        .and_then(|state| state.last_baseline);
    let measured = Measured {
        size: output.size,
        first_baseline: output.first_baselines.y,
        last_baseline: last.or(output.first_baselines.y),
    };
    tree.atomic_memo_mut().insert(key, constraint, measured);
    measured
}

#[cfg(test)]
mod tests {
    use taffy::{AvailableSpace, Size};
    use zgui_arena::{DomainId, Generation};
    use zgui_dom::side::BoxKey;

    use crate::measure::Measured;

    use super::{AtomicMemo, Constraint};

    fn key(index: u32) -> BoxKey {
        BoxKey::new(index, Generation::FIRST, DomainId::FIRST)
    }

    fn constraint(width: AvailableSpace) -> Constraint {
        Constraint::new(
            Size::NONE,
            Size {
                width,
                height: AvailableSpace::MaxContent,
            },
        )
    }

    #[test]
    fn a_second_ask_under_the_same_constraint_is_answered_from_the_memo() {
        let mut memo = AtomicMemo::default();
        let held = Measured::sized(40.0, 20.0);
        assert_eq!(
            memo.get(key(1), constraint(AvailableSpace::MinContent)),
            None
        );
        memo.insert(key(1), constraint(AvailableSpace::MinContent), held);
        assert_eq!(
            memo.get(key(1), constraint(AvailableSpace::MinContent)),
            Some(held)
        );
        assert_eq!(memo.hits(), 1);
        assert_eq!(memo.misses(), 1);
    }

    #[test]
    fn the_three_probes_a_single_measurement_costs_are_three_separate_entries() {
        // Minimum content, maximum content and a definite width are different questions, and an
        // answer to one is not an answer to another.
        let mut memo = AtomicMemo::default();
        let held = Measured::sized(40.0, 20.0);
        memo.insert(key(1), constraint(AvailableSpace::MinContent), held);
        assert_eq!(
            memo.get(key(1), constraint(AvailableSpace::MaxContent)),
            None
        );
        assert_eq!(
            memo.get(key(1), constraint(AvailableSpace::Definite(40.0))),
            None
        );
    }

    #[test]
    fn two_boxes_do_not_share_an_answer() {
        let mut memo = AtomicMemo::default();
        memo.insert(
            key(1),
            constraint(AvailableSpace::MinContent),
            Measured::sized(40.0, 20.0),
        );
        assert_eq!(
            memo.get(key(2), constraint(AvailableSpace::MinContent)),
            None
        );
    }
}
