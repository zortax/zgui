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

/// The pass's tally of shrink-to-fit questions.
///
/// The answers themselves live on each box (see [`AtomicAnswers`]), where the box's own
/// invalidation clears them and an unchanged box keeps them across frames. The tally stays on the
/// pass because it describes the pass: what a test asserts is that *this* layout asked and was
/// answered.
#[derive(Debug, Default)]
pub struct AtomicMemo {
    /// How many nested layouts were avoided.
    hits: u32,
    /// How many were performed.
    misses: u32,
}

impl AtomicMemo {
    /// How many nested layouts were avoided.
    pub fn hits(&self) -> u32 {
        self.hits
    }

    /// How many were performed.
    pub fn misses(&self) -> u32 {
        self.misses
    }
}

/// How many answers one atomic inline keeps.
///
/// Sized the way the size-only ring is: a flex container probes an item at a minimum-content, a
/// maximum-content and a definite constraint per axis pass, so one ring holds a full pass's
/// questions with room over, and the oldest answer makes way past that.
const CAPACITY: usize = 16;

/// The shrink-to-fit answers one atomic inline is holding, across frames.
///
/// Cleared by the box's own
/// [`forget_layout`](crate::tree::store::state::BoxLayout::forget_layout): anything that changes
/// what a nested layout of this box would produce — its style, its content, its descendants'
/// styles, the device scale — marks the box dirty on its way to the root, and dirty is what
/// empties this.
#[derive(Clone, Debug)]
pub(crate) struct AtomicAnswers {
    /// The answers, oldest first.
    entries: [(Constraint, Measured); CAPACITY],
    /// How many entries are filled.
    len: u8,
    /// Where the next answer overwrites once the ring is full.
    next: u8,
}

impl AtomicAnswers {
    /// A held answer, if the box is still holding one for this constraint.
    pub(crate) fn get(&self, constraint: Constraint) -> Option<Measured> {
        self.entries[..self.len as usize]
            .iter()
            .find(|(held, _)| *held == constraint)
            .map(|&(_, answer)| answer)
    }

    /// Records one answer.
    pub(crate) fn insert(&mut self, constraint: Constraint, measured: Measured) {
        if let Some(slot) = self.entries[..self.len as usize]
            .iter_mut()
            .find(|(held, _)| *held == constraint)
        {
            slot.1 = measured;
            return;
        }
        if (self.len as usize) < CAPACITY {
            self.entries[self.len as usize] = (constraint, measured);
            self.len += 1;
            return;
        }
        self.entries[self.next as usize] = (constraint, measured);
        self.next = (self.next + 1) % CAPACITY as u8;
    }

    /// A ring holding one first answer.
    fn holding(constraint: Constraint, measured: Measured) -> Box<Self> {
        Box::new(Self {
            entries: [(constraint, measured); CAPACITY],
            len: 1,
            next: 0,
        })
    }

    /// Forgets every answer. The ring's storage is kept for the next one.
    pub(crate) fn clear(&mut self) {
        self.len = 0;
        self.next = 0;
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
    let held = tree
        .state(key)
        .and_then(|state| state.atomic.as_deref())
        .and_then(|answers| answers.get(constraint));
    if let Some(held) = held {
        tree.atomic_memo_mut().hits += 1;
        return held;
    }
    tree.atomic_memo_mut().misses += 1;
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
    let last = tree.state(key).and_then(|state| state.last_baseline);
    let measured = Measured {
        size: output.size,
        first_baseline: output.first_baselines.y,
        last_baseline: last.or(output.first_baselines.y),
    };
    let state = tree.state_mut(key);
    match state.atomic.as_deref_mut() {
        Some(answers) => answers.insert(constraint, measured),
        None => state.atomic = Some(AtomicAnswers::holding(constraint, measured)),
    }
    measured
}

#[cfg(test)]
mod tests {
    use taffy::{AvailableSpace, Size};

    use crate::measure::Measured;

    use super::{AtomicAnswers, CAPACITY, Constraint};

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
    fn a_second_ask_under_the_same_constraint_is_answered_from_the_ring() {
        let held = Measured::sized(40.0, 20.0);
        let answers = AtomicAnswers::holding(constraint(AvailableSpace::MinContent), held);
        assert_eq!(
            answers.get(constraint(AvailableSpace::MinContent)),
            Some(held)
        );
    }

    #[test]
    fn the_three_probes_a_single_measurement_costs_are_three_separate_entries() {
        // Minimum content, maximum content and a definite width are different questions, and an
        // answer to one is not an answer to another.
        let held = Measured::sized(40.0, 20.0);
        let answers = AtomicAnswers::holding(constraint(AvailableSpace::MinContent), held);
        assert_eq!(answers.get(constraint(AvailableSpace::MaxContent)), None);
        assert_eq!(
            answers.get(constraint(AvailableSpace::Definite(40.0))),
            None
        );
    }

    #[test]
    fn the_ring_is_bounded_and_keeps_the_newest() {
        let mut answers = AtomicAnswers::holding(
            constraint(AvailableSpace::Definite(0.0)),
            Measured::sized(0.0, 0.0),
        );
        for index in 1..CAPACITY * 2 {
            answers.insert(
                constraint(AvailableSpace::Definite(index as f32)),
                Measured::sized(index as f32, 0.0),
            );
        }
        let newest = (CAPACITY * 2 - 1) as f32;
        assert_eq!(
            answers.get(constraint(AvailableSpace::Definite(newest))),
            Some(Measured::sized(newest, 0.0))
        );
        assert_eq!(answers.get(constraint(AvailableSpace::Definite(0.0))), None);
    }

    #[test]
    fn clearing_forgets_every_answer() {
        let held = Measured::sized(40.0, 20.0);
        let mut answers = AtomicAnswers::holding(constraint(AvailableSpace::MinContent), held);
        answers.clear();
        assert_eq!(answers.get(constraint(AvailableSpace::MinContent)), None);
    }
}
