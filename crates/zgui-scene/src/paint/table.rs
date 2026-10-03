//! The interned paint table.

use zgui_color::Color;

use crate::id::PaintId;
use crate::paint::reference::{PaintKind, PaintRef};
use crate::paint::source::Paint;
use crate::table::Table;

/// Every paint source in the document, interned by content and kept across frames.
pub type PaintTable = Table<PaintId, Paint>;

impl PaintTable {
    /// The id of a flat colour.
    pub fn solid(&mut self, color: Color) -> PaintId {
        self.intern(Paint::Solid(color))
    }

    /// The reference a primitive stores for `id`, with the family read off the entry.
    ///
    /// Prefer this to building a [`PaintRef`] by hand: the family and the index are two halves of
    /// one fact, and a shader told "solid" about a gradient reads the wrong storage.
    pub fn reference(&self, id: PaintId) -> PaintRef {
        let kind = match self.get(id) {
            Some(Paint::Solid(_)) => PaintKind::Solid,
            Some(Paint::Gradient { .. }) => PaintKind::Gradient,
            Some(Paint::Image { .. }) => PaintKind::Image,
            None => return PaintRef::NONE,
        };
        PaintRef::new(kind, id)
    }

    /// Interns `paint` and returns the reference a primitive stores for it, in one step.
    pub fn add(&mut self, paint: Paint) -> PaintRef {
        let id = self.intern(paint);
        self.reference(id)
    }

    /// Frees every paint that nothing holds and no recent frame used, and reports how many went.
    ///
    /// A paint is interned by its whole content. A colour in a transition is a new entry on each
    /// frame that draws it, and a gradient in device space is a new entry at each scroll position.
    /// Each such entry costs the table's consumers a slot per walk of the id space. A record holds
    /// what its primitives name, and all other content interns its paint again on each frame that
    /// draws it, so an evicted paint is a cache miss at worst.
    pub fn evict_unused_paints(&mut self, keep_generations: u64, cap: usize) -> usize {
        self.evict_unreachable(keep_generations, cap, |_| None)
    }
}
