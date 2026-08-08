//! The seam between the box that has to be sized and the content that decides how big it is.
//!
//! Three formatting contexts reach the leaf path: a run of text, replaced content this engine does
//! not lay out, and an atomic inline. The third is answered inside this crate, because it is a
//! nested layout of boxes we own. The other two are answered here, by whoever is driving the pass —
//! which is what keeps the shaping engine and the image decoder out of the layout engine's
//! dependencies, and what makes a layout test runnable with no fonts on disk.
//!
//! # Why text is two questions and not one
//!
//! Turning characters into glyphs is expensive; deciding where the lines fall in a given width is
//! cheap. A layout algorithm asks a paragraph how big it is at many candidate widths while it
//! resolves the flex or grid around it, so those questions have to cost the cheap half. The seam
//! therefore has the same shape as the split: [`shape`](MeasureContent::shape) is asked once per
//! distinct content and [`break_lines`](MeasureContent::break_lines) once per width, and a
//! measurer that fused them would turn every width probe back into a full pass.

use taffy::{AvailableSpace, Size};
use zgui_css::ComputedStyle;
use zgui_dom::side::BoxKey;
use zgui_text::{
    BreakRequest, BrokenParagraph, Brush, ContentWidths, ParagraphContent, ParagraphKey,
    StrutMetrics,
};
use zgui_text_style::{TextPaint, TextStyle};

/// What is being asked of the content, and under what constraints.
#[derive(Clone, Copy, Debug)]
pub struct MeasureRequest<'a> {
    /// The box being sized.
    pub box_: BoxKey,
    /// Its computed style.
    pub style: &'a ComputedStyle,
    /// Dimensions the layout engine has already fixed, which are authoritative.
    pub known: Size<Option<f32>>,
    /// The space available on each axis, with the box's own insets already taken off.
    pub available: Size<AvailableSpace>,
    /// The natural size the box's replaced content reported when the box was built, in CSS pixels.
    ///
    /// Carried on the request because the measurer cannot go and ask: the intrinsic lives on the
    /// document, and the document is not the measurer's to reach — a decode thread and a layout
    /// worker meeting on it is exactly the aliasing the seam exists to prevent. `None` for text
    /// and for replaced content that has not said yet.
    pub natural: Option<zgui_geom::Size<zgui_geom::CssPx, zgui_geom::Css>>,
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// Whether the answer will be kept, or is one of several probes taken to find a size.
    ///
    /// A probe may be answered from anything already computed; the kept answer is the one whose
    /// side effects have to be real.
    pub final_pass: bool,
}

/// What the content measured.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Measured {
    /// How big the content is, in device pixels.
    pub size: Size<f32>,
    /// Where its first line's baseline sits, measured down from the top of the content box.
    pub first_baseline: Option<f32>,
    /// Where its last line's baseline sits, measured the same way.
    ///
    /// Carried separately because CSS aligns an inline-block in normal flow on its last line box
    /// and a first-line answer would put a multi-line one on the wrong line.
    pub last_baseline: Option<f32>,
}

impl Measured {
    /// A measurement of the given size with no baseline.
    pub fn sized(width: f32, height: f32) -> Self {
        Self {
            size: Size { width, height },
            first_baseline: None,
            last_baseline: None,
        }
    }

    /// The same measurement with both baselines on one line.
    #[must_use]
    pub fn with_baseline(mut self, baseline: f32) -> Self {
        self.first_baseline = Some(baseline);
        self.last_baseline = Some(baseline);
        self
    }
}

/// What one shaping pass produced, for a caller that never opens the shaped result.
///
/// The intrinsic widths are the point: they are a property of the glyphs alone, so an inline-axis
/// intrinsic probe is answered from here and costs no line breaking at all.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShapedSummary {
    /// The key the shaped result is held under, which every later break names it by.
    pub key: ParagraphKey,
    /// How narrow and how wide the content can be.
    pub widths: ContentWidths,
}

/// Whoever can say how big a box's content is.
pub trait MeasureContent {
    /// Measures one box's replaced content under the given constraints.
    fn measure(&mut self, request: MeasureRequest<'_>) -> Measured;

    /// Shapes one paragraph, or reports the shaped result already held for it.
    fn shape(&mut self, content: &ParagraphContent<'_>) -> ShapedSummary;

    /// The keyed form of [`MeasureContent::shape`], for a flattened context that has already paid
    /// to hash all of its characters.
    fn shape_keyed(&mut self, key: ParagraphKey, content: &ParagraphContent<'_>) -> ShapedSummary {
        let shaped = self.shape(content);
        // A measurer that shaped something has to agree with the caller: the caller has already
        // hashed the content, and two keys for one paragraph means the shaping and every later
        // break are held under different names.
        //
        // A measurer that shaped *nothing* is the one answer allowed to differ. [`NoContent`] — and
        // the runtime's own text engine when no shaper is installed — reports the default key to
        // everything, which is the same "names nothing" key [`MeasureContent::break_lines`] is
        // documented to answer to. Requiring it to agree here would make a measurer that is working
        // exactly as specified trip an assertion on the first paragraph it is asked about.
        debug_assert!(
            shaped.key == key || shaped.key == ParagraphKey::default(),
            "the caller and measurer disagree on the key: the caller says {key:?} and the \
             measurer says {:?}",
            shaped.key
        );
        shaped
    }

    /// Breaks an already shaped paragraph into lines.
    ///
    /// A key that names nothing has no lines, which is what a measurer with no text engine behind
    /// it answers to everything.
    fn break_lines(&mut self, key: ParagraphKey, request: &BreakRequest<'_>) -> BrokenParagraph;

    /// The strut of a block whose root text style is `style`.
    ///
    /// Asked for separately from shaping because a block establishes a strut whether or not it
    /// holds any text, and because every run on a line contributes one of its own.
    fn strut(&mut self, style: &TextStyle) -> StrutMetrics;

    /// The brush slot one run's paint occupies, claiming one if it has none yet.
    ///
    /// A slot is claimed against the identity of the cascade result the colour came from, never
    /// against the colour, because the slot has to survive a theme change that rewrites what is in
    /// it — and two runs that merely computed to the same colour must not be re-coloured together.
    fn paint_slot(&mut self, paint: &TextPaint) -> Brush;

    /// Where each selectable unit of one broken line sits.
    ///
    /// Asked for by `text-overflow`, which needs the boundary a line may be cut on: a cluster is
    /// where a character begins, and the specification hides a character that would only partly
    /// overflow rather than cutting through it. Nothing else in layout asks — geometry is decided
    /// from the line boxes — so a measurer with no shaper behind it answers nothing and is right to.
    fn visit_clusters(
        &self,
        _paragraph: ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(zgui_text::ClusterRun<'_>),
    ) {
    }
}

/// A measurer that reports every box as empty and every paragraph as having no lines.
///
/// Useful for laying out a tree whose leaves are all sized by their own styles, and as the control
/// in a test that has to distinguish "the content decided this" from "the box did".
#[derive(Clone, Copy, Debug, Default)]
pub struct NoContent;

impl MeasureContent for NoContent {
    fn measure(&mut self, _request: MeasureRequest<'_>) -> Measured {
        Measured::default()
    }

    fn shape(&mut self, _content: &ParagraphContent<'_>) -> ShapedSummary {
        ShapedSummary::default()
    }

    fn break_lines(&mut self, _key: ParagraphKey, _request: &BreakRequest<'_>) -> BrokenParagraph {
        BrokenParagraph::default()
    }

    fn strut(&mut self, _style: &TextStyle) -> StrutMetrics {
        StrutMetrics::default()
    }

    fn paint_slot(&mut self, _paint: &TextPaint) -> Brush {
        zgui_scene::PaintSlot(0)
    }
}

/// A measurer that sizes replaced content by the natural size the request carries.
///
/// This is the production answer for a leaf the engine does not lay out: the box was built from
/// the document's intrinsic, the request carries that intrinsic, and the content is exactly as
/// big as it said it was — scaled to device pixels, with any axis the engine already fixed taken
/// as fixed. Content that has reported nothing yet measures empty, which is the truthful size of
/// a picture that has not arrived.
///
/// It shapes no text; pair it with a shaper through
/// [`Paragraphs::with_replaced`](crate::text::Paragraphs::with_replaced).
#[derive(Clone, Copy, Debug, Default)]
pub struct NaturalSize;

impl MeasureContent for NaturalSize {
    fn measure(&mut self, request: MeasureRequest<'_>) -> Measured {
        let natural = request.natural;
        let side =
            |known: Option<f32>,
             axis: fn(&zgui_geom::Size<zgui_geom::CssPx, zgui_geom::Css>) -> f32| {
                known.unwrap_or_else(|| natural.as_ref().map(axis).unwrap_or(0.0) * request.scale)
            };
        Measured::sized(
            side(request.known.width, |size| size.width.0),
            side(request.known.height, |size| size.height.0),
        )
    }

    fn shape(&mut self, _content: &ParagraphContent<'_>) -> ShapedSummary {
        ShapedSummary::default()
    }

    fn break_lines(&mut self, _key: ParagraphKey, _request: &BreakRequest<'_>) -> BrokenParagraph {
        BrokenParagraph::default()
    }

    fn strut(&mut self, _style: &TextStyle) -> StrutMetrics {
        StrutMetrics::default()
    }

    fn paint_slot(&mut self, _paint: &TextPaint) -> Brush {
        zgui_scene::PaintSlot(0)
    }
}

impl zgui_text::ShapedGlyphs for NoContent {
    fn visit_line(
        &self,
        _paragraph: ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(zgui_text::ShapedRun<'_>),
    ) {
    }
}

impl zgui_text::ShapedClusters for NoContent {
    fn visit_clusters(
        &self,
        _paragraph: ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(zgui_text::ClusterRun<'_>),
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::{MeasureContent, Measured, NoContent};

    #[test]
    fn a_baseline_is_absent_until_it_is_given() {
        let measured = Measured::sized(10.0, 20.0);
        assert_eq!(measured.first_baseline, None);
        let with = measured.with_baseline(16.0);
        assert_eq!(with.first_baseline, Some(16.0));
        assert_eq!(with.last_baseline, Some(16.0));
        assert_eq!(with.size, measured.size);
    }

    #[test]
    fn the_empty_measurer_reports_nothing_rather_than_refusing() {
        use taffy::{AvailableSpace, Size};
        use zgui_arena::{DomainId, Generation};
        use zgui_css::StyleDraft;
        use zgui_dom::side::BoxKey;
        use zgui_text::{BreakRequest, ParagraphContent, TextMap};
        use zgui_text_style::ParagraphStyle;

        use super::MeasureRequest;

        fn assert_object_safe(_: &dyn MeasureContent) {}
        assert_object_safe(&NoContent);

        let style = StyleDraft::initial().build();
        let measured = NoContent.measure(MeasureRequest {
            box_: BoxKey::new(1, Generation::FIRST, DomainId::FIRST),
            style: &style,
            known: Size::NONE,
            available: Size {
                width: AvailableSpace::Definite(200.0),
                height: AvailableSpace::MaxContent,
            },
            natural: None,
            scale: 1.0,
            final_pass: true,
        });
        assert_eq!(
            measured.size,
            Size {
                width: 0.0,
                height: 0.0
            }
        );
        assert_eq!(measured.first_baseline, None);
        assert_eq!(measured.last_baseline, None);

        // The text half answers too, rather than being unimplemented: a document laid out with no
        // text engine behind it has paragraphs, and they have no lines.
        let map = TextMap::new();
        let paragraph = ParagraphStyle::initial();
        let content = ParagraphContent {
            text: "some words",
            map: &map,
            runs: &[],
            boxes: &[],
            paragraph: &paragraph,
            scale: 1.0,
        };
        let summary = NoContent.shape(&content);
        let broken = NoContent.break_lines(summary.key, &BreakRequest::new(&content, None));
        assert!(broken.geometry.lines.is_empty());
        assert_eq!(summary.widths.max, zgui_geom::CssPx(0.0));
    }
}
