//! Frame-scoped "already visited" marking that costs nothing to reset.

#[cfg(test)]
mod tests;

/// The stamp value that means "never visited", reserved so a fresh slot reads as unvisited.
const NEVER: u32 = 0;

/// A set of visited indices that is emptied by bumping a counter rather than by clearing memory.
///
/// A walk that must not visit the same node twice needs a "seen" set per walk. Allocating one, or
/// clearing one, costs time proportional to the whole index space even when the walk touched three
/// entries. This set stores an epoch stamp per index instead: emptying it is one increment, and
/// every stamp written under an older epoch is stale by construction.
///
/// The stamps are 32 bits, so the counter eventually wraps. That is handled rather than ignored —
/// the one bump that would wrap clears the stamps, which is the only clearing pass this type ever
/// performs and happens once every four billion epochs.
///
/// ```
/// use zgui_bits::EpochBitset;
///
/// let mut visited = EpochBitset::new();
/// assert!(visited.visit(7));        // first time this epoch
/// assert!(!visited.visit(7));       // already seen
/// assert!(visited.contains(7));
///
/// visited.bump();                   // a new walk begins
/// assert!(!visited.contains(7));
/// assert!(visited.visit(7));
/// ```
#[derive(Clone, Debug)]
pub struct EpochBitset {
    /// The epoch each index was last visited in, or [`NEVER`].
    stamps: Vec<u32>,
    /// The epoch being marked now. Never [`NEVER`].
    epoch: u32,
}

impl EpochBitset {
    /// An empty set in its first epoch.
    pub const fn new() -> Self {
        Self {
            stamps: Vec::new(),
            epoch: NEVER + 1,
        }
    }

    /// An empty set with room for indices below `capacity` before it reallocates.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            stamps: Vec::with_capacity(capacity),
            epoch: NEVER + 1,
        }
    }

    /// The epoch currently being marked.
    ///
    /// Two indices marked in the same epoch were marked by the same walk; a stamp from any earlier
    /// epoch is stale.
    pub const fn epoch(&self) -> u32 {
        self.epoch
    }

    /// Ends the current epoch and begins the next, forgetting every visit.
    ///
    /// This is the whole reset: it does not touch the stamps, except on the one bump in every
    /// 2³² that would otherwise let an ancient stamp be mistaken for a current one.
    pub fn bump(&mut self) {
        match self.epoch.checked_add(1) {
            Some(next) => self.epoch = next,
            None => {
                self.stamps.fill(NEVER);
                self.epoch = NEVER + 1;
            }
        }
    }

    /// Marks `index` visited, returning whether this is the first visit of the current epoch.
    ///
    /// The set grows to fit `index`, so a caller need not size it up front.
    pub fn visit(&mut self, index: usize) -> bool {
        if index >= self.stamps.len() {
            self.stamps.resize(index + 1, NEVER);
        }
        let stamp = &mut self.stamps[index];
        let first = *stamp != self.epoch;
        *stamp = self.epoch;
        first
    }

    /// Whether `index` has been visited in the current epoch, without marking it.
    pub fn contains(&self, index: usize) -> bool {
        self.stamps.get(index).copied() == Some(self.epoch)
    }

    /// Forgets `index`'s visit in the current epoch.
    pub fn forget(&mut self, index: usize) {
        if let Some(stamp) = self.stamps.get_mut(index) {
            *stamp = NEVER;
        }
    }

    /// How many indices the set currently holds a stamp for.
    ///
    /// Visiting an index at or beyond this grows the set; visiting one below it never does. It is
    /// zero until the first visit, whatever was reserved by
    /// [`EpochBitset::with_capacity`], which reserves room without stamping it.
    pub fn capacity(&self) -> usize {
        self.stamps.len()
    }

    /// Forgets every visit and returns to the first epoch, keeping the allocation.
    pub fn reset(&mut self) {
        self.stamps.fill(NEVER);
        self.epoch = NEVER + 1;
    }
}

impl Default for EpochBitset {
    fn default() -> Self {
        Self::new()
    }
}
