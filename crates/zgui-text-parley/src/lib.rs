//! The text engine: shaping, face resolution, metrics, glyph rasterisation and bidirectional text.
//!
//! This is the implementation behind the framework's text contracts, and the only crate in the
//! workspace that names a font engine at all. Everything above it is written against the traits it
//! implements, so it can be replaced, or left out, without a consumer changing.
//!
//! | Contract | Implemented by |
//! |---|---|
//! | [`FontSource`](zgui_text::FontSource) | [`FontSystem`] |
//! | [`FontMetricsSource`](zgui_text::FontMetricsSource) | [`FontSystem`] |
//! | [`ParagraphShaper`](zgui_text::ParagraphShaper) | [`Shaper`] |
//! | [`GlyphRaster`](zgui_text::GlyphRaster) | [`Rasteriser`] |
//!
//! ```
//! use std::sync::Arc;
//! use zgui_geom::CssPx;
//! use zgui_scene::PaintSlot;
//! use zgui_text::{
//!     BreakRequest, FontSource, ParagraphCache, ParagraphContent, StyledRun, TextMap, lay_out,
//! };
//! use zgui_text_parley::{FontSystem, FontSystemOptions, Shaper};
//! use zgui_text_style::{ParagraphStyle, TextStyle};
//!
//! let fonts = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
//! let bytes: Arc<dyn AsRef<[u8]> + Send + Sync> =
//!     Arc::new(std::fs::read("tests/fonts/NotoSans-Regular.ttf").expect("the shipped face"));
//! fonts.register(bytes, None).expect("a readable face");
//!
//! let mut shaper = Shaper::new(fonts.clone());
//! let mut cache = ParagraphCache::new();
//!
//! let style = Arc::new(TextStyle::initial());
//! let paragraph = ParagraphStyle::initial();
//! let text = "the quick brown fox";
//! let mut map = TextMap::new();
//! map.push(0..text.len(), 0, 0);
//! let runs = [StyledRun { text: 0..text.len(), style, brush: PaintSlot(0) }];
//! let content = ParagraphContent {
//!     text,
//!     map: &map,
//!     runs: &runs,
//!     boxes: &[],
//!     paragraph: &paragraph,
//!     scale: 1.0,
//! };
//!
//! let request = BreakRequest::new(&content, Some(CssPx(80.0)));
//! let (_shaped, broken) = lay_out(&mut shaper, &mut cache, &content, &request);
//! assert!(broken.geometry.lines.len() > 1, "eighty pixels does not hold one line of it");
//! ```
//!
//! # Reproducibility, and the mode that provides it
//!
//! What a document looks like depends on which faces are available, and what is installed differs
//! between machines. [`Enumeration::Registered`] is the mode in which nothing is discovered: the
//! collection holds only what was handed to it, so the same registrations produce the same shaped
//! advances and the same pixels anywhere. Every test and every reference image uses it; only a real
//! application enumerates the system.
//!
//! # Base direction
//!
//! A paragraph's base direction is forced by prefixing a **directional mark** — U+200F or U+200E —
//! onto the text the engine is handed, never by wrapping the paragraph in an isolate pair. The
//! bidirectional algorithm's paragraph-level rule skips isolate contents, so a paragraph inside an
//! isolate has no strong character visible to base-level detection at all: its content still
//! reorders correctly, and it then aligns to the wrong edge. See [`Controls`].
//!
//! The prefix belongs to no source position, and **no offset a caller ever sees counts it**. It is
//! added to the string this crate hands the engine and taken back off every offset the engine
//! reports — line ranges, cluster ranges, the map beside the shaped result — so a byte offset means
//! the same thing on both sides of this boundary: an index into the string the caller generated.
//!
//! There is no third space to be converted between, and that is the point. A caret is placed by
//! mapping a model offset to a cluster and drawn from that cluster's box, while a click is resolved
//! by finding the cluster under it and mapping its offset back; if the offsets the engine reports
//! counted a prefix the caller's map does not, the two would be inverse functions of each other and
//! agree with each other perfectly while placing the insertion point some characters away from the
//! caret on the screen.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod direction;
pub mod font;
pub mod metrics;
pub mod raster;
pub mod shape;
pub mod system;

pub use crate::direction::{Controls, LEFT_TO_RIGHT_MARK, RIGHT_TO_LEFT_MARK};
pub use crate::font::{ColorSupport, script_of};
pub use crate::metrics::{BASE_SIZE, MONOSPACE_BASE_SIZE};
pub use crate::raster::Rasteriser;
pub use crate::shape::{LineRequest, ResolvedLineMetrics, ShapedLayout, Shaper, SlotBrush};
pub use crate::system::{Enumeration, FontSystem, FontSystemOptions};

/// A script, as a fallback list is keyed on it.
///
/// Re-exported because [`script_of`] answers in this type and a caller that wants to name a script
/// directly should not have to reach past this crate for the spelling.
pub type Script = fontique::Script;
