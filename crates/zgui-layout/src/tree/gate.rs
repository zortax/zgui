//! Whether a document has to be laid out at all.
//!
//! Laying a document out is the largest thing a frame does, and almost every frame asks for one
//! without needing one: a colour that changed, a caret that blinked and an animation that repainted
//! all leave every box exactly where the previous pass put it. Running the pass anyway is not
//! wasted arithmetic that the caches absorb — the pass has to walk from the root to reach the
//! caches at all, and that walk is proportional to the whole document rather than to what changed.
//!
//! # What the answer is derived from, and why those two things are the whole of it
//!
//! A box's result is a function of the styles and content underneath it and of the space it was
//! given. So a held result stops standing for exactly two reasons:
//!
//! * **something underneath it changed** — which is what invalidating a box's cache means, and
//!   which [`mark_dirty`](crate::tree::dirty::mark_dirty) propagates to the root by construction:
//!   it clears every ancestor up to the first one already cleared, and that one cleared its own
//!   ancestors when it was marked. So *anything* dirty implies the root is dirty, and a clean root
//!   is a document in which nothing is owed;
//! * **the viewport moved**, which no box can see, because the viewport is handed to the pass and
//!   not to the boxes. The store records it, and
//!   [`laid_out_for`](crate::tree::store::LayoutStore::laid_out_for) answers it.
//!
//! A document that fails neither test would be laid out to exactly the numbers it is already
//! holding, so the pass is skipped and the numbers stand.

use taffy::Size;

use crate::tree::dirty::is_dirty;
use crate::tree::store::LayoutStore;

/// What a gated layout pass did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relayout {
    /// There was no box tree to lay out.
    NoRoot,
    /// Nothing was owed and the viewport had not moved, so the results already held are the ones
    /// the pass would have produced and it did not run.
    Held,
    /// The document was laid out.
    Ran,
}

impl Relayout {
    /// Whether there was a document to lay out, whether or not a pass ran for it.
    pub fn had_a_root(self) -> bool {
        self != Self::NoRoot
    }

    /// Whether a pass ran.
    pub fn ran(self) -> bool {
        self == Self::Ran
    }
}

/// Whether the results a store is holding already answer a layout into `viewport`.
///
/// A store with no root answers `false`: there is nothing to hold and nothing to skip, and the
/// caller has a separate answer for that case.
pub fn stands(store: &LayoutStore, viewport: Size<f32>) -> bool {
    let Some(root) = store.root() else {
        return false;
    };
    store.laid_out_for(viewport) && !is_dirty(store, root)
}
