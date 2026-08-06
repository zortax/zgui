//! Computed style and fragment geometry into scene primitives: the stage that decides what a
//! frame draws.
//!
//! Everything above this crate describes a document; everything below it draws rectangles. This is
//! where the two meet, and it produces a [`Scene`](zgui_scene::Scene) — a value — so the whole
//! stage is testable with no graphics device anywhere.
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
//! rather than a thousand. Emission is memoised per fragment: the range of the scene's operation
//! log a fragment occupied is recorded, and next frame an unchanged fragment replays that range —
//! translated, if it merely moved — instead of being encoded again.
//!
//! ```
//! use zgui_geom::Size;
//! use zgui_paint::Painter;
//! use zgui_scene::Scene;
//!
//! let mut painter = Painter::new();
//! let mut scene = Scene::new();
//! scene.begin_frame(Size::new(64, 64));
//!
//! // A document with no boxes emits nothing, and says so rather than leaving it to be inferred.
//! let report = painter.report_of_nothing();
//! assert_eq!(report.primitives, 0);
//! assert!(report.emitted.is_empty());
//! ```

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod shiftable;
pub mod content;
pub mod damage;
pub mod emit;
pub mod lower;
pub mod walk;

pub use crate::content::glyphs::OutlineGlyph;
pub use crate::content::{
    ContentCache, Drawing, FrameContent, ImageError, NoVectors, VectorCache, VectorSource, Vectors,
};
pub use crate::damage::accumulate::{Expansion, expand, vacated};
pub use crate::damage::ink::{ReadExtent, cull_rect, read_extent_of};
pub use crate::emit::highlight::{
    Highlight, HighlightLayer, HighlightRequest, HighlightSource, NoHighlights,
};
pub use crate::emit::text::{
    GlyphRequest, GlyphRun, GlyphSource, NoGlyphs, PlacedGlyph, RunContent,
};
pub use crate::lower::anim::{AnimOverrides, NoAnim};
pub use crate::lower::cache::{PaintStyleCache, PaintStyleRef};
pub use crate::lower::key::LoweringKey;
pub use crate::lower::{PaintStyle, lower};
pub use crate::walk::{PaintInput, PaintReport, Painter};
