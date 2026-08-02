//! Rule 3's overlap test, and the three readings of it.

use zgui_geom::{Device, DevicePx, Rect};

/// A non-vector primitive emitted while a pass was accumulating.
#[derive(Clone, Copy, Debug)]
pub struct Intervening {
    /// What it paints.
    pub bounds: Rect<DevicePx, Device>,
    /// How many of the pass's items had already been accumulated when it was emitted.
    ///
    /// This is what makes the test correct rather than merely conservative. A pass's composite
    /// lands above *every* item of it, so an intervening primitive is only ever hidden by content
    /// that was already accumulated when it was emitted. A primitive that overlaps an item admitted
    /// later is simply that item's background — that item was given an order above it for exactly
    /// that reason — and moving the composite past it changes nothing at all.
    pub accumulated: usize,
}

/// How "an intervening primitive overlaps the accumulated content" is decided.
///
/// The readings agree whenever a pass's items form one blob, and disagree sharply on a grid: a
/// card's own background always meets the *bounding box* of a row of charts, and never meets any
/// chart's own ink. On a twenty-region dashboard with a legend drawn over each chart, the three
/// readings cost twenty, four and one pass respectively.
///
/// The weaker two are kept as selectable policy rather than deleted, because that is what makes the
/// chosen one a measurement instead of an assertion. They cost nothing at run time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overlap {
    /// Test against each earlier item's own ink. The policy.
    ///
    /// O(items in the pass) per candidate, and **exactly** sound rather than approximate: the
    /// scratch is cleared to transparent before a pass writes it, so wherever the pass drew nothing
    /// the composite's source is fully transparent, and compositing a fully transparent
    /// premultiplied texel leaves the destination bit-identical. Ordering can only be violated
    /// where the pass actually has ink.
    #[default]
    PerItemInk,
    /// Test against the bounding box of everything accumulated *before* the primitive.
    ///
    /// O(1) per candidate and conservative: it splits whenever an intervening primitive lands
    /// anywhere in the accumulated extent, including the empty space between two items that are far
    /// apart — which on a grid is most of the extent.
    BoundingBox,
    /// Test against the bounding box of the whole pass, the candidate included, ignoring the order
    /// the primitive was emitted in.
    ///
    /// The literal reading of "overlaps the accumulated bounding box" when the box is taken after
    /// admitting the candidate. It charges an item's own background — drawn immediately *below* the
    /// item, so moving the composite past it cannot change a pixel — as an intervening primitive,
    /// which it never is. It exists so the cost of that reading is a measured number.
    BoundingBoxOrderBlind,
    /// Never split on rule 3 at all.
    ///
    /// Not a legal policy for a pass composited as one draw. It exists to measure what the pass
    /// count would be if each item were composited separately, which is sound exactly when no two
    /// items of a pass overlap each other — the condition
    /// [`PlannedPass::instanced`](crate::PlannedPass::instanced) records.
    Never,
}

impl Overlap {
    /// Whether admitting one more item would trap an intervening primitive under the composite.
    ///
    /// `accumulated` holds the inks of the items already in the pass, in draw order, and
    /// `candidate` is the ink of the item being admitted. Every reading but
    /// [`Overlap::BoundingBoxOrderBlind`] deliberately leaves the candidate out: it is drawn after
    /// every intervening primitive here, so covering one of them is what it is supposed to do.
    pub fn splits(
        self,
        accumulated: &[Rect<DevicePx, Device>],
        candidate: Rect<DevicePx, Device>,
        intervening: &[Intervening],
    ) -> bool {
        match self {
            Self::Never => false,
            Self::BoundingBoxOrderBlind => accumulated
                .iter()
                .copied()
                .chain(core::iter::once(candidate))
                .reduce(Rect::union)
                .is_some_and(|whole| {
                    intervening
                        .iter()
                        .any(|primitive| primitive.bounds.intersects(whole))
                }),
            Self::BoundingBox => intervening.iter().any(|primitive| {
                earlier(accumulated, primitive)
                    .iter()
                    .copied()
                    .reduce(Rect::union)
                    .is_some_and(|box_| primitive.bounds.intersects(box_))
            }),
            Self::PerItemInk => intervening.iter().any(|primitive| {
                earlier(accumulated, primitive)
                    .iter()
                    .any(|ink| primitive.bounds.intersects(*ink))
            }),
        }
    }
}

/// The inks that were already accumulated when `primitive` was emitted.
fn earlier<'a>(
    accumulated: &'a [Rect<DevicePx, Device>],
    primitive: &Intervening,
) -> &'a [Rect<DevicePx, Device>] {
    &accumulated[..primitive.accumulated.min(accumulated.len())]
}
