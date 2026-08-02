//! What the pointer is over.

use smallvec::SmallVec;
use zgui_dom::{Document, NodeKey, StyleFilter};
use zgui_vocab::UiState;

use crate::hit::HitChain;
use crate::state::within::{Moved, move_bit};

/// Which elements are hovered, and the writes that keep that true.
///
/// One of these per pointer that can hover. A finger cannot hover — it is either touching or it is
/// not there — so a touch interaction leaves this empty and a control whose only affordance
/// appears on hover is, correctly, unreachable by touch rather than permanently hovered.
#[derive(Clone, Debug, Default)]
pub struct Hover {
    /// The path currently carrying the bit, root first.
    path: SmallVec<[NodeKey; 8]>,
}

impl Hover {
    /// The elements that are hovered, root first.
    pub fn path(&self) -> &[NodeKey] {
        &self.path
    }

    /// The innermost hovered element.
    pub fn target(&self) -> Option<NodeKey> {
        self.path.last().copied()
    }

    /// Moves the hover onto `chain`, writing only the elements that changed.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn move_to(
        &mut self,
        document: &Document,
        filter: &dyn StyleFilter,
        chain: &HitChain,
    ) -> Moved {
        let moved = move_bit(document, filter, UiState::HOVER, &self.path, chain.path());
        self.path = chain.path().iter().copied().collect();
        moved
    }

    /// Clears the hover, which is what a pointer leaving the surface does.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn clear(&mut self, document: &Document, filter: &dyn StyleFilter) -> Moved {
        let moved = move_bit(document, filter, UiState::HOVER, &self.path, &[]);
        self.path.clear();
        moved
    }
}
