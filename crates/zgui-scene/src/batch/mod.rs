//! Grouping the sorted primitives into the fewest draw calls that preserve their order.

use core::ops::Range;

use crate::id::DrawOrder;
use crate::prim::PrimitiveKind;
use crate::scene::Scene;

/// What the merge orders primitives by: draw order first, then kind.
///
/// The kind is the tie-break, and it is a batching preference rather than a correctness mechanism —
/// two primitives at equal draw order are provably non-overlapping.
type SortKey = (DrawOrder, PrimitiveKind);

/// One draw call's worth of primitives.
///
/// A batch is always a contiguous range of one of the scene's arrays, which is what makes it a
/// memory copy into an instance buffer rather than a gather.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Batch {
    /// Rounded rectangles.
    Quads(Range<usize>),
    /// Box shadows.
    Shadows(Range<usize>),
    /// Text decoration lines.
    Decorations(Range<usize>),
    /// Single-channel coverage sprites reading one texture.
    MonoSprites {
        /// The texture every sprite of the batch reads.
        texture: u32,
        /// The range of the scene's array.
        range: Range<usize>,
    },
    /// Three-channel coverage sprites reading one texture.
    SubpixelSprites {
        /// The texture every sprite of the batch reads.
        texture: u32,
        /// The range of the scene's array.
        range: Range<usize>,
    },
    /// Full-colour sprites reading one texture.
    ColorSprites {
        /// The texture every sprite of the batch reads.
        texture: u32,
        /// The range of the scene's array.
        range: Range<usize>,
    },
    /// One composite of a rasterisation pass, by its index in the pass plan.
    Vector(usize),
    /// One external texture, by its index in the scene's array.
    External(usize),
    /// One backdrop filter, by its index in the scene's array.
    Backdrop(usize),
    /// One group marker, by its index in the scene's array.
    ///
    /// Never merged with anything: a renderer has to change target at exactly this point, and a
    /// marker swallowed into a batch is a target switched at the wrong moment.
    Group(usize),
}

/// The scene's primitives, grouped into draw calls.
///
/// The iterator merges the arrays by `(draw order, primitive kind)` and yields the longest run it
/// can take from one of them before another array's next primitive would have to come first. That
/// is the whole of batching: order is a total order, and a batch is a maximal contiguous run within
/// it.
///
/// Sprites break additionally on a change of texture, because a draw call binds one.
pub struct Batches<'scene> {
    /// The scene being walked.
    scene: &'scene Scene,
    /// How far each array has been consumed, indexed by [`PrimitiveKind`] position.
    cursors: [usize; 11],
}

impl<'scene> Batches<'scene> {
    /// Groups `scene`'s primitives, which must already be finished.
    pub(crate) fn new(scene: &'scene Scene) -> Self {
        Self {
            scene,
            cursors: [0; 11],
        }
    }

    /// The next primitive waiting in `kind`'s array, as a sort key.
    fn peek(&self, kind: PrimitiveKind) -> Option<SortKey> {
        let cursor = self.cursors[kind as usize];
        self.scene.order_at(kind, cursor).map(|order| (order, kind))
    }

    /// The two lowest waiting keys, which is all a merge needs.
    ///
    /// A scan rather than a sort: eleven candidates is small enough that finding the two smallest
    /// in one pass beats sorting all of them, and the second smallest is exactly the bound the
    /// winning run may not cross.
    fn two_lowest(&self) -> (Option<SortKey>, Option<SortKey>) {
        let mut best: Option<SortKey> = None;
        let mut second: Option<SortKey> = None;
        for kind in PrimitiveKind::ALL {
            let Some(candidate) = self.peek(kind) else {
                continue;
            };
            if best.is_none_or(|held| candidate < held) {
                second = best;
                best = Some(candidate);
            } else if second.is_none_or(|held| candidate < held) {
                second = Some(candidate);
            }
        }
        (best, second)
    }
}

impl Iterator for Batches<'_> {
    type Item = Batch;

    fn next(&mut self) -> Option<Batch> {
        let (best, second) = self.two_lowest();
        let (_, kind) = best?;
        let limit = second.unwrap_or((DrawOrder::MAX, PrimitiveKind::GroupEnd));
        let start = self.cursors[kind as usize];
        let (batch, next) = self.scene.take_batch(kind, start, limit);
        self.cursors[kind as usize] = next;
        Some(batch)
    }
}
