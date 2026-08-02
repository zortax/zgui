//! Where a shaped paragraph's selectable units come from, named by paragraph rather than by
//! shaping.

use crate::cluster::run::ClusterRun;
use crate::paragraph::key::ParagraphKey;

/// Where a shaped paragraph's cluster geometry comes from.
///
/// The counterpart of [`ShapedGlyphs`](crate::ShapedGlyphs), and separate from it for the same
/// reason a cluster is separate from a glyph: a glyph is what is painted, a cluster is what can be
/// selected, and on a ligature or a combining mark the two do not correspond. Anything that places
/// a caret, paints a selection or turns a point into a text offset reads this one.
///
/// Also `&self`, and for the same reason: reading where the clusters are is not a write, so a
/// caret can be placed while the engine is measuring something else.
pub trait ShapedClusters {
    /// Visits the direction-uniform cluster runs of one line, in the order they are drawn.
    ///
    /// A paragraph that was never shaped, a line index past the last line, and a line with nothing
    /// on it all visit nothing, which is the same answer and places a caret in the same place.
    fn visit_clusters(
        &self,
        paragraph: ParagraphKey,
        line: u16,
        visit: &mut dyn FnMut(ClusterRun<'_>),
    );
}
