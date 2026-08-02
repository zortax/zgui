//! One document's four sizes, fitted and printed.

use zgui_bench::reference::fit;
use zgui_bench::reference::sample::median;

use super::document::Clipping;
use super::sweep;

/// The sizes the sweep runs over, in rows.
///
/// Every row is three boxes and two runs of text, so the largest document here is seventy thousand
/// boxes — enough that the walk is the frame, and short of the point where building the document
/// costs more than measuring it.
const ROWS: [usize; 4] = [1_250, 2_500, 5_000, 10_000];

/// How many passes are driven at each size before anything is recorded.
const WARMUP: usize = 6;

/// How many are driven with the clock running.
const REPEATS: usize = 16;

/// The least-squares slope of one quantity against boxes the walk reached, in nanoseconds per box.
fn slope(sizes: &[sweep::Point], of: impl Fn(&sweep::Point) -> f64) -> f64 {
    let points: Vec<(f64, f64)> = sizes.iter().map(|size| (size.visited, of(size))).collect();
    fit::slope(&points).expect("four distinct sizes determine a line")
}

/// The smallest and largest of a set of shares.
fn spread(shares: &[f64]) -> (f64, f64) {
    let low = shares.iter().copied().fold(f64::INFINITY, f64::min);
    let high = shares.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (low, high)
}

/// Drives one document at every size and prints what it says.
pub(crate) fn document(clipping: Clipping) {
    let tag = clipping.name();
    let sizes: Vec<sweep::Point> = ROWS
        .into_iter()
        .map(|rows| {
            let point = sweep::at(rows, clipping, WARMUP, REPEATS);
            println!(
                "SIZE {tag} rows={} boxes={} visited={:.0} frames={} walks={} wall_ns={:.0} \
                 fused_ns={:.0} traversal_ns={:.0} faulting_ns={:.0} geometry_ns={:.0} \
                 index_ns={:.0} settle_ns={:.0}",
                point.rows,
                point.boxes,
                point.visited,
                point.frames,
                point.walks,
                point.wall,
                point.fused,
                point.traversal,
                point.faulting,
                point.geometry,
                point.index,
                point.settle,
            );
            point
        })
        .collect();

    let wall = slope(&sizes, |size| size.wall);
    let fused = slope(&sizes, |size| size.fused);
    let traversal = slope(&sizes, |size| size.traversal);
    let faulting = slope(&sizes, |size| size.faulting);
    let geometry = slope(&sizes, |size| size.geometry);
    let index = slope(&sizes, |size| size.index);
    let settle = slope(&sizes, |size| size.settle);
    let telling = index + settle;
    println!("SLOPE {tag} frame {wall:.1} ns/box — the whole pass, walk included");
    println!("SLOPE {tag} walk {fused:.1} ns/box — one fused descent");
    println!("SLOPE {tag} traversal {traversal:.1} ns/box — the recursion and the child lists");
    println!("SLOPE {tag} faulting {faulting:.1} ns/box — memory the walk brings in");
    println!("SLOPE {tag} geometry {geometry:.1} ns/box — the rectangles and the clip chains");
    println!("SLOPE {tag} index {index:.1} ns/box — the hit entries and the moved marks");
    println!("SLOPE {tag} settle {settle:.1} ns/box — the hit index's hierarchy over them");
    println!(
        "SLOPE {tag} parts {:.1} ns/box against a whole pass of {wall:.1}, of which the walk is \
         {fused:.1} and the settle is outside it",
        traversal + faulting + geometry + index + settle,
    );

    let mut every: Vec<f64> = sizes.iter().flat_map(|size| size.shares.clone()).collect();
    let (low, high) = spread(&every);
    let share = median(&mut every);
    println!(
        "SPLIT {tag} telling/(telling+geometry) = {:.3} by slope; {share:.3} median over {} \
         passes, spread {low:.3}..{high:.3}",
        telling / (telling + geometry),
        every.len(),
    );
    println!(
        "SPLIT {tag} geometry {geometry:.1} ns/box against telling {telling:.1} ns/box, of which \
         {index:.1} is in the walk and {settle:.1} is the settle after it"
    );
    println!(
        "SPLIT {tag} without the settle the same comparison reads {:.3}, which is the figure to \
         quote only if repairing the hierarchy an entry's move dirtied is somebody else's cost",
        index / (index + geometry),
    );
}
