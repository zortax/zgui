//! The text contracts: everything the rest of the framework says about text, with no font engine
//! behind any of it.
//!
//! Nothing here shapes a glyph, opens a face or rasterises anything. This crate is the set of seams
//! a text engine is plugged in through, plus the types on both sides of them — so a font engine can
//! be replaced, or absent, without a single consumer changing.
//!
//! | Seam | What plugs in | Who asks |
//! |---|---|---|
//! | [`FontMetricsSource`] | a font system, or [`FixedMetrics`] | the cascade, resolving `ex`, `ch`, `cap` and `ic` |
//! | [`FontSource`] | a font collection | face resolution and `@font-face` |
//! | [`ParagraphShaper`] | a text engine | layout, once per paragraph and many times per width |
//! | [`GlyphRaster`] | a rasteriser | painting, once per distinct glyph |
//!
//! # The one idea everything is arranged around
//!
//! Shaping is expensive and breaking is cheap — on a thousand words, about twenty-eight to one. A
//! layout engine asks a paragraph for its size at many candidate widths while it resolves the flex
//! or grid around it, so those questions must cost breaks.
//!
//! [`ShapedParagraph`] is therefore built once per distinct content-and-style and held in a
//! [`ParagraphCache`]; [`BreakRequest`] carries the width, and
//! [`ShapedParagraph::begin_break`] is the single place that decides whether a pass is owed.
//! [`lay_out`] is the whole protocol in one call.
//!
//! ```
//! use zgui_geom::CssPx;
//! use zgui_scene::PaintSlot;
//! use zgui_text::{BreakRequest, ParagraphContent, ParagraphKey, StyledRun, TextMap};
//! use zgui_text_style::{ParagraphStyle, TextStyle};
//!
//! let style = std::sync::Arc::new(TextStyle::initial());
//! let paragraph = ParagraphStyle::initial();
//! let map = TextMap::new();
//! let runs = [StyledRun { text: 0..5, style, brush: PaintSlot(0) }];
//!
//! let content = ParagraphContent {
//!     text: "hello",
//!     map: &map,
//!     runs: &runs,
//!     boxes: &[],
//!     paragraph: &paragraph,
//!     scale: 1.0,
//! };
//! assert!(content.runs_are_well_formed());
//!
//! // Two different widths are two different breaks of one shape.
//! let narrow = BreakRequest::new(&content, Some(CssPx(100.0))).key();
//! let wide = BreakRequest::new(&content, Some(CssPx(400.0))).key();
//! assert_ne!(narrow, wide);
//! assert_eq!(ParagraphKey::of(&content), ParagraphKey::of(&content));
//! ```
//!
//! # Why a brush is an index
//!
//! [`Brush`] is a slot number in a table that lives as long as the document, never a colour. A
//! shaped paragraph outlives the frame that produced it, so storing a colour would mean a theme
//! change invalidated every one of them; storing an index means the same change is a handful of
//! writes into the table with nothing re-shaped.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod a11y;
pub mod brush;
pub mod cluster;
pub mod font;
pub mod geometry;
pub mod glyph;
pub mod map;
pub mod metrics;
pub mod paragraph;
pub mod transform;

/// Path geometry, re-exported because a glyph's outline is part of this crate's vocabulary.
///
/// A face's curves cross this contract as `kurbo` paths and reach a path rasteriser unchanged, so
/// re-exporting the spelling is what lets an implementor of [`GlyphRaster`] name the geometry it
/// returns without pinning its own copy of a version that might not be this one.
pub use kurbo;

pub use crate::a11y::{ClusterGeometry, TextRunAttributes};
pub use crate::brush::Brush;
pub use crate::cluster::{ClusterRun, ShapedClusters};
pub use crate::font::{FaceId, FaceRecord, FontData, FontError, FontSource};
pub use crate::geometry::{LineGeometry, StrutMetrics, TextGeometry};
pub use crate::glyph::{
    ATLAS_MAX_SIZE, AtlasGlyph, GlyphFormat, GlyphImage, GlyphKey, GlyphOutline, GlyphRaster,
    NoRaster, OutlineKey, PenPosition, RasterPath, RasterStyle, RunProfile, RunSurface,
    SYNTHETIC_BOLD_RATIO, ShapedGlyph, ShapedGlyphs, ShapedRun, ShapedRunOwned, SubpixelOffset,
};
pub use crate::map::{Segment, SourcePos, TextMap};
pub use crate::metrics::{FaceMetrics, FaceQuery, FixedMetrics, FontMetricsSource};
pub use crate::paragraph::{
    BreakRequest, BrokenParagraph, ContentKey, ContentWidths, InlineBoxGeometry,
    InlineBoxPlacement, LineBand, LineBands, MaxAdvance, ParagraphCache, ParagraphContent,
    ParagraphKey, ParagraphShaper, Plan, ShapedParagraph, StyledRun, breaking_key, lay_out,
};
pub use crate::transform::{Language, Transformer};
/// Which way the clusters of a run advance on the screen.
///
/// The accessibility vocabulary's own enumeration, re-exported because it is what
/// [`TextRunAttributes`] reports and what [`ClusterRun`] carries: a second spelling of the same
/// closed set would have to be converted at every boundary and would be one more thing to keep in
/// step.
pub use accesskit::TextDirection;
