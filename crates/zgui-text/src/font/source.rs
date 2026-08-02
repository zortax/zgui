//! The seam a font collection is plugged in through.

use std::sync::Arc;

use smallvec::SmallVec;
use zgui_interned::Ident;
use zgui_text_style::GenericFamily;

use crate::font::error::FontError;
use crate::font::face::{FaceId, FaceRecord};
use crate::metrics::query::FaceQuery;

/// The bytes of one font file, shared rather than copied.
///
/// A font file is between a hundred kilobytes and several megabytes, and the same bytes are read by
/// the shaper, the rasteriser and the metrics source. Passing them by shared reference is what
/// keeps a document with twenty registered faces from holding sixty copies of them.
pub type FontData = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// Where faces come from.
///
/// Two kinds of implementation are expected and the trait serves both: one that enumerates whatever
/// the operating system has installed, and one that knows only the faces the application handed it.
/// The second is what makes a rendering test reproducible, so nothing here may assume the first.
///
/// Every method takes a shared reference. Registering a face mutates the collection, so an
/// implementation locks internally — the alternative would put a mutable borrow of the font system
/// on the path of every style resolution, which runs on several threads at once.
pub trait FontSource: Send + Sync + 'static {
    /// Adds the faces in one font file, under `family` if given and under the file's own family
    /// name otherwise.
    ///
    /// A file may hold several faces — a collection, or a variable face with named instances — so
    /// this returns all of them.
    fn register(
        &self,
        data: FontData,
        family: Option<Ident>,
    ) -> Result<SmallVec<[FaceId; 4]>, FontError>;

    /// Removes every face registered under `family`.
    ///
    /// Anything already shaped keeps its handles and stays drawable; the removal affects what is
    /// resolved next, which is what a style sheet dropping an `@font-face` rule means.
    fn unregister(&self, family: Ident);

    /// The best face for `query`, or nothing if no family in the query has one.
    fn resolve(&self, query: &FaceQuery<'_>) -> Option<FaceId>;

    /// The best face for `query` that can draw `character`.
    ///
    /// Distinct from [`resolve`](FontSource::resolve) because fallback is per character: a run of
    /// Latin text with one emoji in it resolves to two faces, and asking for the run as a whole
    /// cannot express that.
    fn resolve_for(&self, query: &FaceQuery<'_>, character: char) -> Option<FaceId>;

    /// What is known about one face.
    fn face(&self, id: FaceId) -> Option<FaceRecord>;

    /// The family the environment has configured for one generic role.
    fn generic_family(&self, generic: GenericFamily) -> Option<Ident>;
}
