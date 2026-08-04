//! The shaper: one font context, one scratch space, and the two halves of the protocol.

use std::sync::Arc;

use zgui_text::{
    BreakRequest, BrokenParagraph, ParagraphContent, ParagraphShaper, ShapedParagraph, StrutMetrics,
};
use zgui_text_style::TextStyle;

use crate::direction::Controls;
use crate::shape::brush::SlotBrush;
use crate::shape::engine::ShapedLayout;
use crate::shape::strut::StrutCache;
use crate::shape::{breaking, build};
use crate::system::FontSystem;

/// Turns paragraphs into glyphs, and those glyphs into lines.
///
/// One of these belongs to whatever lays text out — it is not shared between threads, because the
/// engine's own scratch buffers and shaping-plan caches are what make a second paragraph in the
/// same style cheap, and they are per-instance.
///
/// ```
/// use zgui_text::{FontSource, ParagraphShaper};
/// use zgui_text_parley::{FontSystem, FontSystemOptions, Shaper};
/// use zgui_text_style::TextStyle;
/// use std::sync::Arc;
///
/// let fonts = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
/// let mut shaper = Shaper::new(fonts.clone());
///
/// // With nothing registered there is no face, and a strut is still well defined.
/// let strut = shaper.strut(&TextStyle::initial());
/// assert_eq!(strut.font_size, TextStyle::initial().size);
/// ```
pub struct Shaper {
    /// The system the collection and the metrics come from.
    fonts: Arc<FontSystem>,
    /// This shaper's view of that collection, plus its own file cache.
    context: parley::FontContext,
    /// The engine's scratch space and shaping-plan caches, reused across paragraphs.
    scratch: parley::LayoutContext<SlotBrush>,
    /// Struts measured so far.
    struts: StrutCache,
    /// How the base direction is forced.
    controls: Controls,
}

impl Shaper {
    /// A shaper drawing on `fonts`, forcing each paragraph's base direction with a directional
    /// mark.
    pub fn new(fonts: Arc<FontSystem>) -> Self {
        Self::with_controls(fonts, Controls::Mark)
    }

    /// A shaper that forces the base direction the given way.
    ///
    /// [`Controls::Verbatim`] leaves the generated string exactly as the caller wrote it, which is
    /// what a caller that has already emitted its own directional controls needs — and what a
    /// caller wanting the base direction detected from the content rather than taken from the
    /// style needs.
    pub fn with_controls(fonts: Arc<FontSystem>, controls: Controls) -> Self {
        let context = fonts.font_context();
        Self {
            fonts,
            context,
            scratch: parley::LayoutContext::new(),
            struts: StrutCache::default(),
            controls,
        }
    }

    /// The system this shaper draws faces from.
    pub fn fonts(&self) -> &Arc<FontSystem> {
        &self.fonts
    }

    /// How this shaper forces the base direction.
    pub fn controls(&self) -> Controls {
        self.controls
    }

    /// Forgets every measured strut, which a newly registered face makes necessary.
    ///
    /// Shaped paragraphs are held by their caller, not here, so this is the whole of what a font
    /// arriving invalidates on this side.
    pub fn forget_measurements(&mut self) {
        self.struts.clear();
    }
}

impl ParagraphShaper for Shaper {
    type Engine = ShapedLayout;

    fn shape(&mut self, content: &ParagraphContent<'_>) -> ShapedParagraph<Self::Engine> {
        self.shape_keyed(zgui_text::ParagraphKey::of(content), content)
    }

    fn shape_keyed(
        &mut self,
        key: zgui_text::ParagraphKey,
        content: &ParagraphContent<'_>,
    ) -> ShapedParagraph<Self::Engine> {
        let strut = match content.runs.first() {
            Some(run) => self.strut(&run.style),
            None => self.strut(&TextStyle::initial()),
        };
        build::shape(
            key,
            content,
            strut,
            self.controls,
            &mut self.context,
            &mut self.scratch,
        )
    }

    fn break_lines(
        &mut self,
        shaped: &mut ShapedParagraph<Self::Engine>,
        request: &BreakRequest<'_>,
    ) -> BrokenParagraph {
        breaking::break_lines(shaped, request)
    }

    fn strut(&mut self, style: &TextStyle) -> StrutMetrics {
        self.struts
            .get_or_measure(style, &mut self.context, &mut self.scratch)
    }

    fn visit_line(
        &self,
        shaped: &ShapedParagraph<Self::Engine>,
        line: u16,
        visit: &mut dyn FnMut(zgui_text::ShapedRun<'_>),
    ) {
        crate::shape::glyphs::visit_line(&self.fonts, &shaped.engine, line, visit);
    }

    fn visit_clusters(
        &self,
        shaped: &ShapedParagraph<Self::Engine>,
        line: u16,
        visit: &mut dyn FnMut(zgui_text::ClusterRun<'_>),
    ) {
        crate::shape::clusters::visit_clusters(&shaped.engine, line, visit);
    }
}

impl core::fmt::Debug for Shaper {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Shaper")
            .field("controls", &self.controls)
            .finish_non_exhaustive()
    }
}
