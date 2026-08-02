//! A bounded set of pairwise disjoint damage rectangles.

mod merge;
mod override_env;
#[cfg(test)]
mod tests;

use zgui_geom::{Device, Rect};

pub use crate::damage_set::override_env::full_damage_forced;

/// How many disjoint rectangles a damage set holds before it starts merging to stay inside itself.
///
/// The number is a trade with a cost on both sides, which is why it is named rather than written
/// as a literal at the one place it is used. Raising it lets a frame that changed several unrelated
/// places redraw each of them separately instead of redrawing their bounding box — but every
/// rectangle is its own render pass, with its own clear, its own scissor and its own set of state
/// changes, and passes are paid for on every frame whether or not the extra precision saved
/// anything. Lowering it merges sooner, so a frame that touched two corners of the surface redraws
/// the whole of it.
///
/// Four is where restricting the redraw still deletes real work and the pass count is still
/// negligible beside it.
///
/// The rule for changing it is in `CONTRIBUTING.md`, and it is a measurement rather than an
/// argument: a change to this number is a change to how many passes every frame in the workspace
/// costs, so it is made with the scenario evidence that a different number is better and not
/// without.
pub const MAX_DAMAGE: usize = 4;

/// A bounded set of **pairwise disjoint** rectangles covering everything that must be redrawn.
///
/// The set holds at most `N` rectangles. Adding one that intersects an existing rectangle merges
/// the two; adding one beyond the capacity merges the pair whose union wastes the least area. So
/// pathological scatter degrades gracefully to a single bounding rectangle rather than growing
/// without bound, and no pixel is ever covered twice.
///
/// Disjointness is a requirement, not a tidiness preference: each rectangle is redrawn in its own
/// pass, so two overlapping rectangles clear and shade the shared pixels twice and pay for two
/// passes to do it. `N` defaults to [`MAX_DAMAGE`], which is where restricting the redraw still
/// saves real work and the pass count stays negligible.
///
/// A set can also be *full*, meaning the whole surface must be redrawn — a resize, a scale change,
/// a device loss, a theme swap, or the very first frame. A full set holds no rectangles: what the
/// surface's bounds are is the caller's knowledge, not the set's.
///
/// ```
/// use zgui_bits::DamageSet;
/// use zgui_geom::{Device, Point, Rect, Size};
///
/// let mut damage = DamageSet::<4>::new();
/// damage.absorb(Rect::new(Point::new(0, 0), Size::new(10, 10)));
/// damage.absorb(Rect::new(Point::new(5, 5), Size::new(10, 10)));
/// // The two overlapped, so one rectangle now contains both.
/// assert_eq!(damage.len(), 1);
/// assert!(damage.rects()[0].contains_rect(Rect::new(Point::new(0, 0), Size::new(15, 15))));
///
/// let far: Rect<i32, Device> = Rect::new(Point::new(500, 500), Size::new(4, 4));
/// damage.absorb(far);
/// assert_eq!(damage.len(), 2);
/// assert!(damage.intersects(far));
/// ```
#[derive(Clone, Copy)]
pub struct DamageSet<const N: usize = MAX_DAMAGE> {
    /// The covered rectangles. Only the first `len` entries are meaningful.
    rects: [Rect<i32, Device>; N],
    /// How many entries of `rects` are in use.
    len: usize,
    /// Set when the whole surface must be redrawn.
    full: bool,
}

impl<const N: usize> DamageSet<N> {
    /// How many rectangles the set can hold before it starts merging to stay inside its bound.
    pub const CAPACITY: usize = N;

    /// Refuses a capacity of zero at compile time: a set that can hold nothing has nowhere to
    /// merge to and could not keep its own promise.
    const CAPACITY_IS_USABLE: () = assert!(N >= 1);

    /// An empty set: nothing is damaged.
    pub const fn new() -> Self {
        () = Self::CAPACITY_IS_USABLE;
        Self {
            rects: [Rect::ZERO; N],
            len: 0,
            full: false,
        }
    }

    /// A set covering the whole surface.
    pub const fn full() -> Self {
        let mut set = Self::new();
        set.full = true;
        set
    }

    /// The set a frame should start from: empty, unless [`full_damage_forced`] says otherwise.
    ///
    /// Reaching for this rather than [`DamageSet::new`] is what makes the environment override
    /// work, and the override is the first thing to try when a visual artefact is reported.
    pub fn for_frame() -> Self {
        if full_damage_forced() {
            Self::full()
        } else {
            Self::new()
        }
    }

    /// Whether the whole surface must be redrawn.
    pub const fn is_full(&self) -> bool {
        self.full
    }

    /// Declares that the whole surface must be redrawn, discarding the individual rectangles.
    pub fn set_full(&mut self) {
        self.full = true;
        self.len = 0;
    }

    /// Whether nothing at all needs redrawing.
    pub const fn is_empty(&self) -> bool {
        !self.full && self.len == 0
    }

    /// How many rectangles the set holds. Always zero when the set is full.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// The rectangles, pairwise disjoint and in no particular order.
    pub fn rects(&self) -> &[Rect<i32, Device>] {
        &self.rects[..self.len]
    }

    /// Cuts every rectangle down to `surface`, dropping the ones that lie wholly outside it.
    ///
    /// The rectangles reaching this are the union of what moved, and what moves during a scroll is
    /// the whole of a document that is mostly not on the screen: a page twice the height of the
    /// window damages twice the window. Nothing outside the surface can be redrawn — the renderer
    /// cuts the set to the surface before it clears anything — so carrying those pixels any further
    /// buys nothing and costs the emit walk the subtree skip it exists for: every test against the
    /// damage passes, so the walk descends the whole document and paints, into the void, everything
    /// hanging off the sides of it.
    ///
    /// A full set is left alone: it already means the surface and nothing more.
    pub fn clip_to(&mut self, surface: Rect<i32, Device>) {
        if self.full {
            return;
        }
        let mut kept = 0;
        for index in 0..self.len {
            if let Some(cut) = self.rects[index].intersection(surface)
                && !cut.is_empty()
            {
                self.rects[kept] = cut;
                kept += 1;
            }
        }
        self.len = kept;
    }

    /// Empties the set, and clears the full-surface flag with it.
    pub fn clear(&mut self) {
        self.len = 0;
        self.full = false;
    }

    /// Unions `rect` in and merges into it every existing rectangle it touches, so afterwards a
    /// single rectangle of the set contains all of `rect`.
    ///
    /// Empty rectangles are ignored, and a full set stays full and stays as it is.
    ///
    /// When the set is already at capacity, the pair whose union wastes the least area is merged
    /// and the union re-absorbed, so the merge is transitively closed: `A ∪ B` can meet a third
    /// rectangle that lay between them and touched neither, and pushing the union without
    /// re-absorbing it would leave two overlapping rectangles in a set that promises none.
    /// Each such re-entry strictly decreases the rectangle count before growing it back by one,
    /// so it cannot run away.
    ///
    /// A rectangle a held one already contains is absorbed by doing nothing, which is not merely an
    /// optimisation of the merge loop: it is the case a scroll produces thousands of times per
    /// frame, once for every piece of a list that moved inside a scrollport already damaged whole.
    pub fn absorb(&mut self, rect: Rect<i32, Device>) {
        if self.full || rect.is_empty() {
            return;
        }
        if self.contains(rect) {
            return;
        }
        let mut merged = rect;
        let mut index = 0;
        while index < self.len {
            if self.rects[index].intersects(merged) {
                merged = merged.union(self.rects[index]);
                self.remove(index);
                // The union grew, so a rectangle already passed over may now meet it.
                index = 0;
            } else {
                index += 1;
            }
        }
        self.push(merged);
    }

    /// Absorbs every rectangle of `other`, and becomes full if `other` is.
    pub fn absorb_set(&mut self, other: &Self) {
        if other.full {
            self.set_full();
            return;
        }
        for rect in other.rects() {
            self.absorb(*rect);
        }
    }

    /// Whether one held rectangle covers the whole of `rect`, so that absorbing it would change
    /// nothing.
    ///
    /// Answered against single rectangles rather than against the set's coverage, because the set
    /// is disjoint: a rectangle spanning two of them is covered by neither and genuinely has to be
    /// merged in for the two to become one.
    pub fn contains(&self, rect: Rect<i32, Device>) -> bool {
        if self.full {
            return true;
        }
        self.rects().iter().any(|held| held.contains_rect(rect))
    }

    /// Whether `rect` shares a pixel with anything that must be redrawn.
    ///
    /// A full set intersects everything; an empty rectangle intersects nothing.
    pub fn intersects(&self, rect: Rect<i32, Device>) -> bool {
        if self.full {
            return !rect.is_empty();
        }
        self.rects().iter().any(|held| held.intersects(rect))
    }

    /// The smallest rectangle containing the whole set, or `None` when it is empty or full.
    pub fn bounds(&self) -> Option<Rect<i32, Device>> {
        if self.full || self.len == 0 {
            return None;
        }
        Some(
            self.rects()
                .iter()
                .fold(Rect::ZERO, |accumulated, rect| accumulated.union(*rect)),
        )
    }

    /// The number of device pixels the set covers, or `None` when the whole surface is damaged
    /// and the count is therefore the surface's to know.
    ///
    /// Because the rectangles are disjoint, this is a sum and not an estimate.
    pub fn area(&self) -> Option<i64> {
        if self.full {
            return None;
        }
        Some(
            self.rects()
                .iter()
                .map(|rect| merge::area(*rect))
                .fold(0i64, i64::saturating_add),
        )
    }

    /// Appends `rect`, making room by merging the least wasteful pair if the set is at capacity.
    ///
    /// `rect` must not intersect anything already held.
    fn push(&mut self, rect: Rect<i32, Device>) {
        if self.len < N {
            self.rects[self.len] = rect;
            self.len += 1;
            return;
        }

        let (left, right) = merge::least_wasted_pair(&self.rects[..self.len], rect);
        if right == self.len {
            let union = self.rects[left].union(rect);
            self.remove(left);
            self.absorb(union);
        } else {
            let union = self.rects[left].union(self.rects[right]);
            self.remove(right);
            self.remove(left);
            self.push(rect);
            self.absorb(union);
        }
    }

    /// Removes the rectangle at `index`, keeping the rest in order.
    fn remove(&mut self, index: usize) {
        self.rects.copy_within(index + 1..self.len, index);
        self.len -= 1;
    }
}

impl<const N: usize> Default for DamageSet<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Compares the rectangles in use, ignoring whatever is left in the unused tail of the storage.
///
/// The comparison is *order-sensitive*, so it is stricter than "covers the same pixels": two sets
/// holding the same rectangles in a different order are not equal. Compare
/// [`DamageSet::rects`] as a set, or [`DamageSet::area`], where coverage rather than
/// representation is the question.
impl<const N: usize> PartialEq for DamageSet<N> {
    fn eq(&self, other: &Self) -> bool {
        self.full == other.full && self.rects() == other.rects()
    }
}

impl<const N: usize> Eq for DamageSet<N> {}

impl<const N: usize> core::fmt::Debug for DamageSet<N> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("DamageSet")
            .field("full", &self.full)
            .field("rects", &self.rects())
            .finish()
    }
}
