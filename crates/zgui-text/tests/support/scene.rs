//! One paragraph, ready to be laid out at whatever width a test proposes.

use std::sync::Arc;

use zgui_geom::CssPx;
use zgui_scene::PaintSlot;
use zgui_text::{
    BreakRequest, BrokenParagraph, InlineBoxGeometry, ParagraphCache, ParagraphContent, StyledRun,
    TextMap, lay_out,
};
use zgui_text_style::{ParagraphStyle, TextStyle};

use crate::support::mono::{MonoLayout, MonoShaper};

/// A paragraph and everything laying it out needs.
pub(crate) struct Scene {
    /// The generated string.
    pub(crate) text: String,
    /// The map back to the source.
    pub(crate) map: TextMap,
    /// The runs.
    pub(crate) runs: Vec<StyledRun>,
    /// The atomic inlines.
    pub(crate) boxes: Vec<InlineBoxGeometry>,
    /// The paragraph's own properties.
    pub(crate) paragraph: ParagraphStyle,
    /// Device pixels per CSS pixel.
    pub(crate) scale: f32,
    /// The shaper.
    pub(crate) shaper: MonoShaper,
    /// The shaped results held across calls, which is what makes a re-break possible at all.
    pub(crate) cache: ParagraphCache<MonoLayout>,
}

impl Scene {
    /// One paragraph of plain text in one style.
    pub(crate) fn plain(text: &str, style: TextStyle) -> Self {
        let mut map = TextMap::new();
        map.push(0..text.len(), 0, 0);
        Self {
            runs: vec![StyledRun {
                text: 0..text.len(),
                style: Arc::new(style),
                brush: PaintSlot(0),
            }],
            text: text.to_owned(),
            map,
            boxes: Vec::new(),
            paragraph: ParagraphStyle::initial(),
            scale: 1.0,
            shaper: MonoShaper::default(),
            cache: ParagraphCache::new(),
        }
    }

    /// The content, borrowed for one call.
    pub(crate) fn content(&self) -> ParagraphContent<'_> {
        ParagraphContent {
            text: &self.text,
            map: &self.map,
            runs: &self.runs,
            boxes: &self.boxes,
            paragraph: &self.paragraph,
            scale: self.scale,
        }
    }

    /// Lays the paragraph out at one width as a probe: an answer that will not be kept.
    pub(crate) fn probe(&mut self, width: Option<CssPx>) -> BrokenParagraph {
        let content = ParagraphContent {
            text: &self.text,
            map: &self.map,
            runs: &self.runs,
            boxes: &self.boxes,
            paragraph: &self.paragraph,
            scale: self.scale,
        };
        let request = BreakRequest::new(&content, width).probing();
        let (_, broken) = lay_out(&mut self.shaper, &mut self.cache, &content, &request);
        broken
    }

    /// Lays the paragraph out at one width, shaping only if this is new content.
    pub(crate) fn run(&mut self, width: Option<CssPx>) -> BrokenParagraph {
        let content = ParagraphContent {
            text: &self.text,
            map: &self.map,
            runs: &self.runs,
            boxes: &self.boxes,
            paragraph: &self.paragraph,
            scale: self.scale,
        };
        let request = BreakRequest::new(&content, width);
        let (_, broken) = lay_out(&mut self.shaper, &mut self.cache, &content, &request);
        broken
    }
}
