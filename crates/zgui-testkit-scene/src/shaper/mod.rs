//! `MonoShaper`: a paragraph shaper with no font files behind it.
//!
//! It is the second implementation of the shaping contract, and the reason it exists is that the
//! first one cannot be asserted against. A real text engine's answers depend on which faces are
//! installed, which version of them, and how the system enumerates them — so a layout test written
//! against it measures the machine it runs on. This one measures a fixed face: **every character is
//! one cluster, 8 wide and 16 tall at the initial font size**, taken from the same
//! [`FixedMetrics`](zgui_text::FixedMetrics) the cascade resolves `ex` and `ch` against.
//!
//! No file is opened, no directory is scanned and no font library is linked, so the suite runs in a
//! container with no fonts on it at all.
//!
//! # What it is not
//!
//! It is not a typographic engine and must never grow into one. There is no bidirectional
//! reordering, no ligature, no kerning and no fallback: a test that needs any of those needs the
//! real engine, and a test that does not needs an answer it can compute by hand.
//!
//! What it *does* honour is everything a layout test depends on: the cost protocol (shaping is
//! counted once per content, breaking once per distinct request), line breaking at spaces,
//! alignment and indent, the strut, atomic inline boxes with their `vertical-align` shift, and the
//! paragraph's base direction as far as alignment sees it.
//!
//! ```
//! use std::sync::Arc;
//! use zgui_geom::CssPx;
//! use zgui_scene::PaintSlot;
//! use zgui_text::{BreakRequest, ParagraphCache, ParagraphContent, StyledRun, TextMap, lay_out};
//! use zgui_text_style::{ParagraphStyle, TextStyle};
//! use zgui_testkit_scene::MonoShaper;
//!
//! let text = "aaa bbb";
//! let map = TextMap::new();
//! let runs = [StyledRun {
//!     text: 0..text.len(),
//!     style: Arc::new(TextStyle::initial()),
//!     brush: PaintSlot(0),
//! }];
//! let paragraph = ParagraphStyle::initial();
//! let content = ParagraphContent {
//!     text,
//!     map: &map,
//!     runs: &runs,
//!     boxes: &[],
//!     paragraph: &paragraph,
//!     scale: 1.0,
//! };
//!
//! let mut shaper = MonoShaper::new();
//! let mut cache = ParagraphCache::new();
//!
//! let request = BreakRequest::new(&content, Some(CssPx(40.0)));
//! let (_, broken) = lay_out(&mut shaper, &mut cache, &content, &request);
//!
//! assert_eq!(broken.geometry.lines.len(), 2, "eight pixels a cluster, so it wraps");
//! assert_eq!(shaper.shapes(), 1);
//!
//! // A second width is a second break of the same shape, never a second shape.
//! let wider = BreakRequest::new(&content, Some(CssPx(400.0)));
//! lay_out(&mut shaper, &mut cache, &content, &wider);
//! assert_eq!(shaper.shapes(), 1);
//! assert_eq!(shaper.breaks(), 2);
//! ```

pub mod assemble;
pub mod breaking;
pub mod cluster;
pub mod clusters;
pub mod glyphs;
pub mod metrics;

use std::sync::LazyLock;

use zgui_text::{
    BreakRequest, BrokenParagraph, ClusterRun, ContentWidths, ParagraphContent, ParagraphKey,
    ParagraphShaper, ShapedParagraph, ShapedRun, StrutMetrics,
};
use zgui_text_style::TextStyle;

pub use crate::shaper::cluster::{Cluster, MonoLayout};
pub use crate::shaper::glyphs::{MonoRaster, glyph_id};

/// The style a paragraph with no runs at all is measured with.
static FALLBACK: LazyLock<TextStyle> = LazyLock::new(TextStyle::initial);

/// A deterministic shaper with a fixed face and no font files.
///
/// It counts its own passes as well as bumping the framework's counters, which is what lets a test
/// check the two against each other: a shaper that reported a cache hit while performing a shape
/// would satisfy either count alone.
#[derive(Clone, Debug, Default)]
pub struct MonoShaper {
    /// How many paragraphs have been shaped.
    shapes: u64,
    /// How many breaking passes have actually run.
    breaks: u64,
}

impl MonoShaper {
    /// A shaper with nothing shaped yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many shaping passes have run.
    pub fn shapes(&self) -> u64 {
        self.shapes
    }

    /// How many breaking passes have run.
    ///
    /// Lower than the number of calls whenever a request repeats one already reflected in the
    /// glyphs, which is the cheap path the whole protocol exists for.
    pub fn breaks(&self) -> u64 {
        self.breaks
    }

    /// Forgets both counts.
    pub fn reset(&mut self) {
        self.shapes = 0;
        self.breaks = 0;
    }
}

impl ParagraphShaper for MonoShaper {
    type Engine = MonoLayout;

    fn shape(&mut self, content: &ParagraphContent<'_>) -> ShapedParagraph<Self::Engine> {
        debug_assert!(
            content.runs_are_well_formed(),
            "the runs must cover the string in ascending order with no gaps"
        );
        self.shapes += 1;
        let clusters = cluster::shape(content);
        let (min, max) = cluster::content_widths(&clusters);
        let style = content
            .runs
            .first()
            .map_or(&*FALLBACK, |run| run.style.as_ref());

        ShapedParagraph::new(
            ParagraphKey::of(content),
            content.text.to_owned(),
            content.map.clone(),
            ContentWidths { min, max },
            metrics::strut(style),
            content.boxes.iter().copied(),
            MonoLayout {
                scale: content.scale,
                clusters,
                lines: Vec::new(),
                geometry: Vec::new(),
            },
        )
    }

    fn break_lines(
        &mut self,
        shaped: &mut ShapedParagraph<Self::Engine>,
        request: &BreakRequest<'_>,
    ) -> BrokenParagraph {
        // `begin_break` is the single place a pass is decided on: it answers false when the glyphs
        // already reflect exactly this request, and re-breaking anyway is how a width probe turns
        // back into a full pass.
        if shaped.begin_break(request) {
            self.breaks += 1;
            shaped.engine.lines = breaking::into_lines(&shaped.engine.clusters, request);
        }
        let broken = assemble::geometry(shaped, request);
        shaped.engine.geometry = broken.geometry.lines.clone();
        broken
    }

    fn strut(&mut self, style: &TextStyle) -> StrutMetrics {
        metrics::strut(style)
    }

    fn visit_line(
        &self,
        shaped: &ShapedParagraph<Self::Engine>,
        line: u16,
        visit: &mut dyn FnMut(ShapedRun<'_>),
    ) {
        glyphs::visit_line(shaped, line, visit);
    }

    fn visit_clusters(
        &self,
        shaped: &ShapedParagraph<Self::Engine>,
        line: u16,
        visit: &mut dyn FnMut(ClusterRun<'_>),
    ) {
        clusters::visit_clusters(shaped, line, visit);
    }
}
