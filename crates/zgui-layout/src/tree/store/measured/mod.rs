//! The size-only measurements one box has already answered.
//!
//! This is the layout engine's sole size-only cache. It keys the complete question instead of
//! Taffy's former nine-slot approximation, so two probes that differ in a sizing input never share
//! an answer.
//!
//! That is fine for a box asked a handful of questions and ruinous for a box inside a grid. Grid
//! track sizing measures every item at min-content and again at max-content, and it does so once
//! per pass of an algorithm that runs several passes per axis, with a grid area estimate that moves
//! between them — so one item is asked ten questions that all have the same *shape* and different
//! numbers, each evicting the last. Every eviction is a whole nested layout of that item's subtree,
//! re-entered from the top, for an answer the box computed a moment earlier. Measured on a document
//! of 1 851 boxes, one keystroke drove **17 971** box layouts for **12** boxes that had actually
//! changed.
//!
//! It is keyed on the whole of the engine's question (see [`probe`]) and lives behind
//! [`BoxLayout::forget_layout`](crate::tree::store::state::BoxLayout::forget_layout) rather than
//! being cleared by whoever happens to remember.
//!
//! # Why it is bounded
//!
//! A window being dragged asks a new question of every box on every frame, and the answers to the
//! old ones stay right for ever without ever being asked again. So the memo is a ring of a fixed
//! size and the oldest answer makes way. Nothing is lost by that beyond a measurement having to be
//! taken again, which is what would have happened anyway.

pub(crate) mod probe;

use taffy::LayoutInput;

use crate::tree::store::measured::probe::{Answer, Probe};

/// How many answers one box keeps.
///
/// Sized for the questions a grid item is asked in one pass — a min-content and a max-content
/// probe per axis, per track-sizing pass, plus the definite ones the final placement asks — with
/// room over. Past that the oldest is dropped, which costs one measurement and never an answer.
const CAPACITY: usize = 16;

/// The size-only answers one box is holding.
#[derive(Debug, Default)]
pub(crate) struct Measured {
    /// The questions and their answers, oldest first.
    entries: Vec<(Probe, Answer)>,
    /// Where the next answer overwrites once the ring is full.
    next: usize,
}

impl Measured {
    /// The answer this box gave to the same question, if it is still holding it.
    pub(crate) fn get(&self, input: &LayoutInput) -> Option<Answer> {
        let probe = Probe::of(input);
        self.entries
            .iter()
            .find(|(held, _)| *held == probe)
            .map(|&(_, answer)| answer)
    }

    /// Records the answer to one question.
    pub(crate) fn insert(&mut self, input: &LayoutInput, answer: Answer) {
        let probe = Probe::of(input);
        if let Some(slot) = self.entries.iter_mut().find(|(held, _)| *held == probe) {
            slot.1 = answer;
            return;
        }
        if self.entries.len() < CAPACITY {
            self.entries.push((probe, answer));
            return;
        }
        self.entries[self.next] = (probe, answer);
        self.next = (self.next + 1) % CAPACITY;
    }

    /// Whether this box is holding no answer at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Forgets every answer.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.next = 0;
    }
}

#[cfg(test)]
mod tests;
