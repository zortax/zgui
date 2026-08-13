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
    ShapedClusters, StrutMetrics,
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
///
/// # Why the cluster seam is a supertrait
///
/// `text-overflow` needs the boundary a line may be cut on, which is a cluster, so a measurer has
/// to report cluster geometry as well as sizes. A defaulted method here would let a *forwarding*
/// measurer inherit the empty answer a shaperless one gives, and nothing detects that: sizes stay
/// right, the paragraph still paints, and only the cut moves to the line's start edge, so every
/// clipped label draws its mark and none of its words.
///
/// [`ShapedClusters`] has no default answer, so a wrapper either forwards it or does not compile.
pub trait MeasureContent: ShapedClusters {
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

    /// A measurer a parallel batch worker may own, when this one can produce them.
    ///
    /// The default is `None`, which keeps every batch this measurer serves serial. A measurer
    /// that forks must guarantee a fork's answers equal its own, and take the fork's results back
    /// through [`MeasureContent::absorb_measurer`] so later stages read them where they read
    /// everything else.
    ///
    /// `owned` names the shaped paragraphs the worker's requests will break: a forking measurer
    /// moves their entries into the fork, because breaking mutates an entry and the worker has to
    /// own what it mutates. A key the measurer does not hold is shaped fresh on the worker.
    fn fork_measurer(&mut self, owned: &[ParagraphKey]) -> Option<Box<dyn WorkerMeasure>> {
        let _ = owned;
        None
    }

    /// Takes back a measurer handed out by [`MeasureContent::fork_measurer`].
    ///
    /// Called once per fork after a batch commits, in request order.
    fn absorb_measurer(&mut self, worker: Box<dyn WorkerMeasure>) {
        let _ = worker;
    }
}

/// A measurer one batch worker owns for the duration of one batch.
///
/// The `Any` seam is how the measurer that produced a fork recognises it at absorb time: the
/// executor carries forks as trait objects, and only the producer knows the concrete type the
/// results have to be drained out of.
pub trait WorkerMeasure: MeasureContent + Send {
    /// This measurer, for the producer's downcast.
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any;

    /// The same, by value, so the producer can keep the fork warm for the next batch.
    fn into_any(self: Box<Self>) -> Box<dyn core::any::Any>;
}

impl<T: MeasureContent + Send + 'static> WorkerMeasure for T {
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn core::any::Any> {
        self
    }
}

impl ShapedClusters for Box<dyn WorkerMeasure> {
    fn visit_clusters(
        &self,
        paragraph: ParagraphKey,
        line: u16,
        visit: &mut dyn FnMut(zgui_text::ClusterRun<'_>),
    ) {
        (**self).visit_clusters(paragraph, line, visit);
    }
}

impl MeasureContent for Box<dyn WorkerMeasure> {
    fn measure(&mut self, request: MeasureRequest<'_>) -> Measured {
        (**self).measure(request)
    }

    fn shape(&mut self, content: &ParagraphContent<'_>) -> ShapedSummary {
        (**self).shape(content)
    }

    fn shape_keyed(&mut self, key: ParagraphKey, content: &ParagraphContent<'_>) -> ShapedSummary {
        (**self).shape_keyed(key, content)
    }

    fn break_lines(&mut self, key: ParagraphKey, request: &BreakRequest<'_>) -> BrokenParagraph {
        (**self).break_lines(key, request)
    }

    fn strut(&mut self, style: &TextStyle) -> StrutMetrics {
        (**self).strut(style)
    }

    fn paint_slot(&mut self, paint: &TextPaint) -> Brush {
        (**self).paint_slot(paint)
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

    fn fork_measurer(&mut self, _owned: &[ParagraphKey]) -> Option<Box<dyn WorkerMeasure>> {
        // Stateless, so a fork is a copy and absorbing it back is nothing. This is what lets a
        // text-free fixture exercise parallel batches.
        Some(Box::new(Self))
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

impl ShapedClusters for NoContent {
    fn visit_clusters(
        &self,
        _paragraph: ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(zgui_text::ClusterRun<'_>),
    ) {
    }
}

/// Nothing was shaped here, so there is no cluster to report.
impl ShapedClusters for NaturalSize {
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
