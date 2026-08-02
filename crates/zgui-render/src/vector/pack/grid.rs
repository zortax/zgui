//! Which cells of which scratch layer a frame's passes have already claimed.

/// The side, in device pixels, of one cell.
///
/// Pass regions are aligned outwards onto a sixteen-pixel grid before any rasteriser sees one, so at
/// this size a cell is either wholly inside a region or wholly outside it and the overlap test is
/// exact. A region that arrived unaligned rounds outwards here as well, which can only report an
/// overlap that is not there — one more layer than was needed, never two passes on top of each
/// other.
pub const CELL: i32 = 16;

/// A rectangle of cells, half-open on the right and the bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// The first column.
    pub left: usize,
    /// One past the last column.
    pub right: usize,
    /// The first row.
    pub top: usize,
    /// One past the last row.
    pub bottom: usize,
}

impl Span {
    /// Whether the span covers no cell at all.
    pub fn is_empty(self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }
}

/// One bit per cell per layer: set where some pass has already claimed that cell of that layer.
#[derive(Debug)]
pub struct Grid {
    /// Cells across one layer.
    columns: usize,
    /// Cells down one layer.
    rows: usize,
    /// Words of bits one layer costs.
    words: usize,
    /// The layers, end to end, `words` apiece.
    bits: Vec<u64>,
}

impl Grid {
    /// A grid covering `width` by `height` device pixels, with no layers open yet.
    pub fn new(width: i32, height: i32) -> Self {
        let columns = cells(width);
        let rows = cells(height);
        Self {
            columns,
            rows,
            words: (columns * rows).div_ceil(64).max(1),
            bits: Vec::new(),
        }
    }

    /// How many layers are open.
    pub fn layers(&self) -> usize {
        self.bits.len() / self.words
    }

    /// The cells a device-pixel rectangle covers, clamped to the grid.
    pub fn span(&self, left: i32, top: i32, right: i32, bottom: i32) -> Span {
        Span {
            left: (left.max(0) / CELL) as usize,
            right: cells(right).min(self.columns),
            top: (top.max(0) / CELL) as usize,
            bottom: cells(bottom).min(self.rows),
        }
    }

    /// Opens one more layer, and answers with its index.
    pub fn open(&mut self) -> usize {
        let layer = self.layers();
        self.bits.resize(self.bits.len() + self.words, 0);
        layer
    }

    /// Claims `span` in `layer`, or answers `false` having changed nothing because it was taken.
    pub fn claim(&mut self, layer: usize, span: Span) -> bool {
        let base = layer * self.words;
        let Some(words) = self.bits.get_mut(base..base + self.words) else {
            return false;
        };
        for row in span.top..span.bottom {
            let start = row * self.columns + span.left;
            let end = row * self.columns + span.right;
            if any(words, start, end) {
                return false;
            }
        }
        for row in span.top..span.bottom {
            let start = row * self.columns + span.left;
            let end = row * self.columns + span.right;
            set(words, start, end);
        }
        true
    }
}

/// How many cells a device-pixel extent spans, rounded outwards.
fn cells(pixels: i32) -> usize {
    (pixels.max(0) as usize).div_ceil(CELL as usize)
}

/// Whether any bit in `start..end` is set.
fn any(words: &[u64], start: usize, end: usize) -> bool {
    walk(start, end, |word, mask| words[word] & mask != 0)
}

/// Sets every bit in `start..end`.
fn set(words: &mut [u64], start: usize, end: usize) {
    walk(start, end, |word, mask| {
        words[word] |= mask;
        false
    });
}

/// Visits each word `start..end` touches with the mask of the bits it contributes, stopping at the
/// first visit that answers `true`.
fn walk(start: usize, end: usize, mut visit: impl FnMut(usize, u64) -> bool) -> bool {
    let mut at = start;
    while at < end {
        let word = at / 64;
        let stop = ((word + 1) * 64).min(end);
        if visit(word, mask(at - word * 64, stop - word * 64)) {
            return true;
        }
        at = stop;
    }
    false
}

/// Bits `low..high` of one word.
fn mask(low: usize, high: usize) -> u64 {
    let width = high - low;
    if width >= 64 {
        u64::MAX
    } else {
        ((1u64 << width) - 1) << low
    }
}

#[cfg(test)]
mod tests {
    use super::Grid;

    #[test]
    fn a_cell_claimed_once_cannot_be_claimed_again_in_the_same_layer() {
        let mut grid = Grid::new(64, 64);
        let layer = grid.open();
        let span = grid.span(0, 0, 16, 16);
        assert!(grid.claim(layer, span));
        assert!(!grid.claim(layer, span));
    }

    #[test]
    fn a_refused_claim_leaves_the_layer_exactly_as_it_was() {
        let mut grid = Grid::new(64, 64);
        let layer = grid.open();
        assert!(grid.claim(layer, grid.span(0, 0, 16, 16)));
        // Overlaps the first cell and reaches three more; the refusal must not have marked those
        // three, or the next pass to ask for them would be told they were taken.
        assert!(!grid.claim(layer, grid.span(0, 0, 32, 32)));
        assert!(grid.claim(layer, grid.span(16, 0, 32, 32)));
    }

    #[test]
    fn two_layers_claim_the_same_cells_independently() {
        let mut grid = Grid::new(64, 64);
        let first = grid.open();
        let second = grid.open();
        let span = grid.span(0, 0, 48, 48);
        assert!(grid.claim(first, span));
        assert!(grid.claim(second, span));
    }

    #[test]
    fn a_claim_spanning_several_words_is_still_exact() {
        // Twenty columns of cells, so one row alone is not a whole word and a row's bits straddle
        // word boundaries at different offsets — which is where an off-by-one in the masks shows.
        let mut grid = Grid::new(320, 320);
        let layer = grid.open();
        assert!(grid.claim(layer, grid.span(0, 0, 320, 160)));
        assert!(!grid.claim(layer, grid.span(304, 144, 320, 176)));
        assert!(grid.claim(layer, grid.span(0, 160, 320, 320)));
    }
}
