//! What is being pressed.

use smallvec::SmallVec;
use zgui_dom::{Document, NodeKey, StyleFilter};
use zgui_vocab::UiState;

use crate::hit::HitChain;
use crate::state::within::{Moved, move_bit};

/// Which elements are being pressed, and the writes that keep that true.
///
/// Like hover, this is a path and not one element: pressing a button presses the toolbar it is in
/// as far as `:active` is concerned, which is what lets a container style itself while anything
/// inside it is held down.
///
/// The press is released by the pointer coming up **and** by the interaction being cancelled. Both
/// matter: a gesture recogniser claiming a drag, or a window losing its input, ends an interaction
/// with no release at all, and a control that only cleared this on a release stays stuck down
/// until the next press somewhere else.
#[derive(Clone, Debug, Default)]
pub struct Active {
    /// The path currently carrying the bit, root first.
    path: SmallVec<[NodeKey; 8]>,
}

impl Active {
    /// The elements being pressed, root first.
    pub fn path(&self) -> &[NodeKey] {
        &self.path
    }

    /// The innermost pressed element.
    pub fn target(&self) -> Option<NodeKey> {
        self.path.last().copied()
    }

    /// Whether anything is being pressed.
    pub fn is_pressed(&self) -> bool {
        !self.path.is_empty()
    }

    /// Presses `chain`.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn press(
        &mut self,
        document: &Document,
        filter: &dyn StyleFilter,
        chain: &HitChain,
    ) -> Moved {
        let moved = move_bit(document, filter, UiState::ACTIVE, &self.path, chain.path());
        self.path = chain.path().iter().copied().collect();
        moved
    }

    /// Releases whatever is pressed.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn release(&mut self, document: &Document, filter: &dyn StyleFilter) -> Moved {
        let moved = move_bit(document, filter, UiState::ACTIVE, &self.path, &[]);
        self.path.clear();
        moved
    }
}
