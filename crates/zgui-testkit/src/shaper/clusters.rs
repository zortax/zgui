//! The selectable units of a line, from the fixed face.
//!
//! One cluster per character, laid out left to right: this shaper reorders nothing, so a line is
//! always exactly one run. What it does honour is the coordinate the contract states — an offset
//! is measured from the line box's own start edge, which is where the first cluster of the line
//! sits and not where the paragraph starts.

use zgui_geom::CssPx;
use zgui_text::{ClusterGeometry, ClusterRun, ShapedParagraph, TextDirection};

use crate::shaper::cluster::MonoLayout;

/// Visits the one cluster run of one line.
pub(crate) fn visit_clusters(
    shaped: &ShapedParagraph<MonoLayout>,
    line: u16,
    visit: &mut dyn FnMut(ClusterRun<'_>),
) {
    let engine = &shaped.engine;
    let Some((start, end)) = engine.lines.get(line as usize).copied() else {
        return;
    };
    let mut clusters: Vec<ClusterGeometry> = Vec::new();
    let mut pen = 0.0;
    let text = shaped.text();
    for (index, cluster) in engine.clusters[start.min(end)..end].iter().enumerate() {
        // A cluster covers the bytes up to the next one, and the last one covers the rest of the
        // line — which is the end of the text on the last line, and the start of the next line's
        // first cluster otherwise.
        let next = engine
            .clusters
            .get(start + index + 1)
            .map_or(text.len(), |next| next.offset);
        clusters.push(ClusterGeometry {
            text: cluster.offset..next,
            offset: CssPx(pen),
            advance: cluster.advance,
        });
        pen += cluster.advance.0;
    }
    if clusters.is_empty() {
        return;
    }
    visit(ClusterRun {
        direction: TextDirection::LeftToRight,
        start: CssPx(0.0),
        clusters: &clusters,
    });
}
