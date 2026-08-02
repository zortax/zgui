//! One document size, driven in all three shapes of the walk, interleaved.

use zgui_bench::reference::sample::median;
use zgui_layout::fragment::diff::split::Passes;

use super::document::{self, Clipping, Opened};
use super::measure::{self, Drive, nanos};

/// What one document size reported.
pub(crate) struct Point {
    /// How many rows the document holds.
    pub(crate) rows: usize,
    /// How many boxes it laid out.
    pub(crate) boxes: usize,
    /// Boxes the offsetting walk reached over one pass, median.
    pub(crate) visited: f64,
    /// Wall-clock nanoseconds of one pass with the walk untimed and undivided, median.
    pub(crate) wall: f64,
    /// Nanoseconds inside the fused descent over one pass, median.
    pub(crate) fused: f64,
    /// Nanoseconds inside the bare traversal over one pass, median, over boxes already brought in.
    pub(crate) traversal: f64,
    /// Nanoseconds of memory the walk faults in over one pass, median.
    pub(crate) faulting: f64,
    /// Nanoseconds the rectangles and the clip chains cost over one pass, median.
    pub(crate) geometry: f64,
    /// Nanoseconds the hit entries and the accessibility marks cost, median.
    pub(crate) index: f64,
    /// Nanoseconds repairing the hit index's hierarchy cost, median.
    pub(crate) settle: f64,
    /// The index half's share of the two duties, one figure per divided pass.
    pub(crate) shares: Vec<f64>,
    /// Frames one pass drew.
    pub(crate) frames: usize,
    /// Subtrees one pass moved.
    pub(crate) walks: u64,
}

/// Drives one size and reports it.
///
/// The three shapes are driven inside one turn, one after the other, rather than in three blocks of
/// turns. A machine that slows down half way through — a thermal step, another process arriving —
/// slows all three down together, and the shape of the answer is a comparison between them.
pub(crate) fn at(rows: usize, clipping: Clipping, warmup: usize, repeats: usize) -> Point {
    let mut open = document::opened(rows, clipping);
    let boxes = document::boxes(&open.harness);
    assert!(
        boxes >= rows,
        "a {rows}-row document laid out {boxes} boxes, which is fewer than one per row — so this \
         is not the document the probe is about",
    );
    for turn in 0..warmup {
        drive_all(&mut open, turn);
    }
    let mut plain = Vec::new();
    let mut fused = Vec::new();
    let mut apart = Vec::new();
    for turn in 0..repeats {
        let (one, two, three) = drive_all(&mut open, turn + warmup);
        plain.push(one);
        fused.push(two);
        apart.push(three);
    }
    let shares: Vec<f64> = apart.iter().filter_map(|pass| pass.index_share()).collect();
    assert!(
        !shares.is_empty(),
        "no divided pass over {rows} rows produced a share, so the offsetting walk never ran and \
         there is nothing to split",
    );
    let last = apart.last().expect("repeats is not zero");
    let walked_together = median(&mut plain.iter().map(|pass| pass.visited).collect::<Vec<_>>());
    let walked_apart = median(&mut apart.iter().map(|pass| pass.visited).collect::<Vec<_>>());
    assert!(
        (walked_together - walked_apart).abs() < walked_apart * 0.02,
        "the divided walk reached {walked_apart} boxes where the fused walk reached \
         {walked_together}, so the two are not doing the same work over the same document and no \
         difference between their timings means anything",
    );
    Point {
        rows,
        boxes,
        visited: median(&mut apart.iter().map(|pass| pass.visited).collect::<Vec<_>>()),
        wall: median(&mut plain.iter().map(|pass| pass.wall).collect::<Vec<_>>()),
        fused: median(
            &mut fused
                .iter()
                .map(|pass| nanos(pass.spent.together))
                .collect::<Vec<_>>(),
        ),
        traversal: median(
            &mut apart
                .iter()
                .map(|pass| pass.traversal())
                .collect::<Vec<_>>(),
        ),
        faulting: median(&mut apart.iter().map(|pass| pass.faulting()).collect::<Vec<_>>()),
        geometry: median(&mut apart.iter().map(|pass| pass.geometry()).collect::<Vec<_>>()),
        index: median(&mut apart.iter().map(|pass| pass.index()).collect::<Vec<_>>()),
        settle: median(&mut apart.iter().map(|pass| pass.settle()).collect::<Vec<_>>()),
        shares,
        frames: last.frames,
        walks: last.spent.walks,
    }
}

/// One turn: the same gesture in all three shapes of the walk.
fn drive_all(open: &mut Opened, turn: usize) -> (Drive, Drive, Drive) {
    let plain = measure::drive(open, Passes::Together, turn);
    let fused = measure::drive(open, Passes::TogetherTimed, turn);
    let apart = measure::drive(open, Passes::Apart, turn);
    (plain, fused, apart)
}
