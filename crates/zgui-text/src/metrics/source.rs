//! The seam the cascade reads face metrics through.

use zgui_geom::CssPx;
use zgui_text_style::GenericFamily;

use crate::metrics::face::FaceMetrics;
use crate::metrics::query::FaceQuery;

/// Supplies the face metrics the cascade needs to resolve font-relative units.
///
/// `ex`, `ch`, `cap` and `ic` cannot be resolved without opening a face, so computing a style
/// depends on the font system — while the font system must not depend on the style engine, or
/// neither could be replaced. This trait is the seam between them, and it is the whole of it.
///
/// # What an implementation must promise
///
/// Calls arrive from several threads at once, through a shared reference, because styles are
/// cascaded in parallel. An implementation that needs exclusive access to a font collection
/// therefore holds its own lock, and memoises across calls so that the overwhelming majority never
/// take it. Two calls with equal arguments must return equal metrics for as long as the set of
/// registered faces is unchanged; a cascade that saw two answers for one query would produce two
/// different computed styles for one element.
pub trait FontMetricsSource: Send + Sync + 'static {
    /// Metrics for the first face matching `query`, at `size`.
    ///
    /// `vertical` asks for the metrics of the face's vertical writing mode, which differ from the
    /// horizontal ones in more than orientation.
    fn face_metrics(&self, query: &FaceQuery<'_>, size: CssPx, vertical: bool) -> FaceMetrics;

    /// The default size for one generic family.
    ///
    /// This is what an unstyled document starts from, and it is per-family because a monospace
    /// face at the same nominal size reads smaller than a proportional one, so environments
    /// configure the two separately.
    fn base_size(&self, generic: GenericFamily) -> CssPx;
}
