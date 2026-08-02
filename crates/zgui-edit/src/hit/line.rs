//! One line's clusters, kept in the order they are drawn.

use zgui_geom::CssPx;
use zgui_text::{ClusterGeometry, ClusterRun, LineGeometry, TextDirection};

/// One direction-uniform run of a line, owned.
///
/// The shaper hands its runs out by reference for the length of one call, and a hit test outlives
/// that call: a click arrives long after the frame that laid the text out. So a line map keeps its
/// own copy, which is also what lets it be built once and asked many times.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// Which way the clusters advance on the screen.
    pub direction: TextDirection,
    /// Where the run's leading edge sits, from the line box's start edge.
    pub start: CssPx,
    /// The clusters, in logical order.
    pub clusters: Vec<ClusterGeometry>,
}

impl Run {
    /// The run as the shaper reported it.
    pub fn of(run: &ClusterRun<'_>) -> Self {
        Self {
            direction: run.direction,
            start: run.start,
            clusters: run.clusters.to_vec(),
        }
    }

    /// Whether the clusters are drawn right to left.
    pub fn is_rtl(&self) -> bool {
        matches!(self.direction, TextDirection::RightToLeft)
    }

    /// The advance the run occupies.
    pub fn advance(&self) -> f32 {
        self.clusters
            .iter()
            .map(|cluster| cluster.advance.0)
            .sum::<f32>()
    }

    /// The cluster whose box contains `x`, measured from the run's leading edge, and how far into
    /// that box the point fell.
    ///
    /// A point before the run answers with its first cluster and a point after it with its last,
    /// because a caller has already decided this is the run it wants and a hit has to land
    /// somewhere.
    pub fn cluster_at(&self, x: f32) -> Option<(&ClusterGeometry, f32)> {
        let mut nearest: Option<&ClusterGeometry> = None;
        for cluster in &self.clusters {
            let start = cluster.offset.0;
            let end = start + cluster.advance.0;
            if x >= start && x < end {
                return Some((cluster, x - start));
            }
            nearest = match nearest {
                Some(held) if distance(held, x) <= distance(cluster, x) => Some(held),
                _ => Some(cluster),
            };
        }
        nearest.map(|cluster| (cluster, x - cluster.offset.0))
    }
}

/// How far `x` is from a cluster's box.
fn distance(cluster: &ClusterGeometry, x: f32) -> f32 {
    let start = cluster.offset.0;
    let end = start + cluster.advance.0;
    if x < start {
        start - x
    } else {
        (x - end).max(0.0)
    }
}

/// One line: where it sits, and what is on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Where the line box is, relative to the paragraph.
    pub geometry: LineGeometry,
    /// The runs, in the order they are drawn.
    pub runs: Vec<Run>,
}

impl Line {
    /// The run whose box contains `x`, measured from the line box's start edge.
    ///
    /// A point past either end answers with the run at that end, because a click in a line's empty
    /// right-hand margin belongs to that line and has to resolve to an offset on it.
    pub fn run_at(&self, x: f32) -> Option<&Run> {
        let mut nearest: Option<&Run> = None;
        for run in &self.runs {
            let start = run.start.0;
            let end = start + run.advance();
            if x >= start && x < end {
                return Some(run);
            }
            let gap = if x < start { start - x } else { x - end };
            nearest = match nearest {
                Some(held) if gap_to(held, x) <= gap => Some(held),
                _ => Some(run),
            };
        }
        nearest
    }
}

/// How far `x` is from a run's box.
fn gap_to(run: &Run, x: f32) -> f32 {
    let start = run.start.0;
    let end = start + run.advance();
    if x < start {
        start - x
    } else {
        (x - end).max(0.0)
    }
}
