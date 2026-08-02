//! Reading the selectable units of a line out of a broken layout.
//!
//! A cluster is what a caret may be placed beside and what a hit test resolves to, and it is not a
//! glyph: a ligature draws one glyph over several clusters and a mark draws a glyph over none. So
//! this walk is the engine's own cluster walk rather than a second reading of the glyph walk.
//!
//! # Where a run starts
//!
//! The engine measures a cluster's visual offset from the paragraph's origin plus the line's
//! alignment offset, while a line box starts at that alignment offset plus the line's inline
//! minimum coordinate. Subtracting the alignment offset therefore puts a cluster in the line box's
//! own space — the same space the positioned glyphs are reported in — and it does so through the
//! engine's own accounting, which is what keeps an atomic inline sitting between two runs from
//! shifting every cluster after it.
//!
//! # Which string a cluster's range indexes
//!
//! The caller's, always. The engine shaped a string with a directional control in front of the
//! caller's text, so every range it reports is that many bytes further along than the caller's own
//! offsets; the prefix is taken back off here, and a cluster made entirely of it is not a cluster
//! the caller has a character for and is dropped.
//!
//! This is the one place a caret's two directions can be made to disagree while both look right in
//! isolation. A click is resolved by finding the cluster under it and reporting that cluster's
//! range; the caret is drawn by finding the cluster whose range holds an offset. Ranges that
//! counted a prefix the caller's map does not send both through the same shift, in opposite
//! directions — so the caret is painted exactly where it was clicked and the letter typed lands a
//! prefix's worth of bytes away from it.

use zgui_geom::CssPx;
use zgui_text::{ClusterGeometry, ClusterRun, TextDirection};

use crate::shape::brush::SlotBrush;
use crate::shape::engine::ShapedLayout;

/// Visits the direction-uniform cluster runs of one line, in the order they are drawn.
pub(crate) fn visit_clusters(
    shaped: &ShapedLayout,
    line: u16,
    visit: &mut dyn FnMut(ClusterRun<'_>),
) {
    let Some(line) = shaped.layout.get(line as usize) else {
        return;
    };
    let alignment = line.metrics().offset;

    // One buffer for the whole line: a line of prose is one run, and a bidirectional one is a
    // handful, so a buffer per run would allocate once per direction change for nothing.
    let mut clusters: Vec<ClusterGeometry> = Vec::new();
    for run in line.runs() {
        clusters.clear();
        let Some(start) = run_start(&run, alignment) else {
            continue;
        };
        let rtl = run.is_rtl();
        let advance = run.advance();
        let mut before = 0.0;
        for cluster in run.clusters() {
            let width = cluster.advance();
            let text = cluster.text_range();
            let leading = if rtl {
                advance - before - width
            } else {
                before
            };
            before += width;
            if text.end <= shaped.prefix {
                continue;
            }
            clusters.push(ClusterGeometry {
                text: text.start.max(shaped.prefix) - shaped.prefix..text.end - shaped.prefix,
                offset: CssPx(leading),
                advance: CssPx(width),
            });
        }
        if clusters.is_empty() {
            continue;
        }
        visit(ClusterRun {
            direction: if rtl {
                TextDirection::RightToLeft
            } else {
                TextDirection::LeftToRight
            },
            start: CssPx(start),
            clusters: &clusters,
        });
    }
}

/// Where a run's leading edge sits, measured from the line box's start edge.
///
/// Asked of the engine rather than accumulated over the preceding runs, because what sits between
/// two runs is not always another run: an atomic inline occupies the line without being one, and a
/// walk that only added up run advances would place every run after it too far to the left.
fn run_start(run: &parley::Run<'_, SlotBrush>, alignment: f32) -> Option<f32> {
    let first = run.visual_clusters().next()?;
    Some(first.visual_offset()? - alignment)
}
