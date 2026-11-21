//! Computed style and fragment geometry into compositor chunks: the stage that decides what a
//! frame draws.
//!
//! Everything above this crate describes a document; everything below it draws rectangles. This is
//! where the two meet. Each fragment's painting is recorded as compositor items into a transaction
//! through a [`CompositorSync`], against the clip, paint and coordinate-system [`Tables`]. Both are
//! values, so the whole stage is testable with no graphics device anywhere.
//!
//! # The stage runs in two halves, and the order between them is load-bearing
//!
//! [`expand`] grows the frame's damage over the registry of fragments that *read* pixels outside
//! the ones they write — a blur, a drop shadow, a `backdrop-filter` — and it walks that registry
//! rather than the fragment tree. Then, with the damage set frozen, [`Painter::emit`] walks the
//! document in painting order and emits only the fragments the damage reaches.
//!
//! Folding the expansion into the emit walk does not work, and the reason is worth stating: the
//! emit walk skips a whole clean subtree in constant time when the subtree's ink misses the damage,
//! and a fragment's read extent is deliberately not part of that ink. A blurred panel whose own
//! pixels are untouched, sitting over content that is animating, is skipped at an ancestor — and
//! the region its blur samples is cleared by the renderer and repainted by nobody. Expanding first,
//! over a registry with no such gate, closes that hole; emitting second against a frozen set closes
//! the other one, because a rectangle added after the walk is cleared and never redrawn.
//!
//! [`vacated`] is the third rectangle, and it is neither: a subtree that was removed leaves pixels
//! behind that no living fragment can report, so the roots a frame removed are read while their
//! geometry still exists and their ink absorbed before anything else runs.
//!
//! # What a frame costs
//!
//! Lowering is memoised on the identity of the shared computed-value groups a style is made of
//! ([`PaintStyleCache`]), so a thousand identically styled buttons lower a handful of paint styles
//! rather than a thousand. Emission is memoised per fragment: an unchanged fragment keeps its
//! compositor chunk and costs nothing, a moved one has the chunk's offset written, and only a
//! changed one is encoded and recorded again.
//!
//! ```
//! use zgui_geom::Size;
//! use zgui_paint::{Painter, Tables};
//!
//! let mut painter = Painter::new();
//! let mut tables = Tables::new();
//! tables.begin_frame(Size::new(64, 64));
//!
//! // A document with no boxes emits nothing, and says so rather than leaving it to be inferred.
//! let report = painter.report_of_nothing();
//! assert_eq!(report.primitives, 0);
//! assert!(report.emitted.is_empty());
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod content;
pub mod damage;
pub mod emit;
pub mod lower;
pub mod paints;
pub mod parity;
pub mod shiftable;
pub mod sync;
pub mod tables;
pub mod walk;

pub use crate::content::vectors::VectorId;
pub use crate::content::{
    ContentCache, Drawing, FrameContent, ImageError, NoVectors, VectorCache, VectorSource, Vectors,
};
pub use crate::damage::accumulate::{Expansion, expand, vacated};
pub use crate::damage::ink::{ReadExtent, cull_rect, read_extent_of};
pub use crate::emit::highlight::{
    Highlight, HighlightLayer, HighlightRequest, HighlightSource, NoHighlights,
};
pub use crate::emit::text::{GlyphSource, NoGlyphs};
pub use crate::emit::vector::{VectorRoute, VectorRoutes};
pub use crate::lower::anim::{AnimOverrides, NoAnim, OwnedPaints};
pub use crate::lower::cache::{PaintStyleCache, PaintStyleRef};
pub use crate::lower::key::LoweringKey;
pub use crate::lower::{PaintStyle, lower};
pub use crate::sync::{CompositorSync, SyncReport, record_chunk};
pub use crate::tables::Tables;
pub use crate::walk::{PaintInput, PaintReport, Painter, VectorRouteReport};
/// The brush slot a run names.
pub use zgui_space::PaintSlot;
/// The shaped-text vocabulary the glyph seams speak, from the text layer.
///
/// Re-exported because a caller that shapes its own runs and draws them through
/// [`ScenePainter::glyphs`](crate::content::custom::ScenePainter::glyphs) names these types, and
/// should not have to reach past this crate for the spelling.
pub use zgui_text::{Brush, FaceId, ShapedGlyph, ShapedRun, ShapedRunOwned};
