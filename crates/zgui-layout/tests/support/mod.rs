//! Building a styled document, laying it out, and watching what the leaves were asked.
//!
// The harness is compiled into several test targets and each uses a different part of it.
#![allow(dead_code, unreachable_pub)]

pub(crate) mod mono;
pub(crate) mod text;

use std::sync::Arc;

use zgui_dom::host::{Intrinsic, ReplacedContent, ReplacedId};
use zgui_dom::node::flags::NodeFlags;
use zgui_dom::{Document, NodeIndex, NodeKind};
use zgui_geom::CssPx;
use zgui_interned::{ClassName, ElementName};
use zgui_layout::measure::{MeasureContent, MeasureRequest, Measured};
use zgui_layout::style::DeviceStyle;
use zgui_layout::text::Paragraphs;
use zgui_layout::tree::LayoutTree;
use zgui_layout::tree::store::LayoutStore;
use zgui_style::{SheetOrigin, SheetSource, StyleEngine, Viewport};
use zgui_text::FixedMetrics;

/// One element of a fixture tree.
pub(crate) struct Element {
    /// The element's name, which selectors match on.
    pub(crate) name: &'static str,
    /// Its classes.
    pub(crate) classes: &'static [&'static str],
    /// Its text content, appended as a text node.
    pub(crate) text: Option<&'static str>,
    /// Its natural size, when its content comes from outside the document.
    pub(crate) replaced: Option<(f32, f32)>,
    /// Its children.
    pub(crate) children: Vec<Element>,
}

impl Element {
    /// An element with nothing but a name.
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            classes: &[],
            text: None,
            replaced: None,
            children: Vec::new(),
        }
    }

    /// The same element, with classes.
    pub(crate) fn classes(mut self, classes: &'static [&'static str]) -> Self {
        self.classes = classes;
        self
    }

    /// The same element, with a text child.
    pub(crate) fn text(mut self, text: &'static str) -> Self {
        self.text = Some(text);
        self
    }

    /// The same element, replaced by content of the given natural size.
    pub(crate) fn image(mut self, width: f32, height: f32) -> Self {
        self.replaced = Some((width, height));
        self
    }

    /// The same element, with children.
    pub(crate) fn children(mut self, children: Vec<Element>) -> Self {
        self.children = children;
        self
    }
}

/// A styled document, ready to have a box tree built from it.
pub(crate) struct Fixture {
    /// The document.
    pub(crate) document: Document,
    /// The root element.
    pub(crate) root: NodeIndex,
    /// The engine, held because it owns the rule set the document was styled against.
    engine: StyleEngine,
    /// The installed sheets' handles, held because dropping one removes its sheet.
    sheets: Vec<zgui_style::SheetHandle>,
}

impl Fixture {
    /// Builds a document from `tree` and cascades `css` over it.
    pub(crate) fn new(tree: Element, css: &str) -> Self {
        Self::with_natural_size(tree, css, (0.0, 0.0))
    }

    /// The same, with every replaced node reporting one natural size.
    pub(crate) fn with_natural_size(tree: Element, css: &str, natural: (f32, f32)) -> Self {
        let mut document = Document::new();
        document.install_replaced_content(Arc::new(FixedIntrinsic {
            size: zgui_geom::Size::new(CssPx(natural.0), CssPx(natural.1)),
        }));
        let document_index = document.document_index();
        let root = append(&mut document, document_index, &tree);
        let mut engine = StyleEngine::new(
            &document,
            Arc::new(FixedMetrics::new()),
            Viewport::new(CssPx(1280.0), CssPx(800.0)),
        );
        let (handle, diagnostics) =
            engine.add_sheet(&document, SheetOrigin::Author, SheetSource::Text(css));
        assert!(
            diagnostics.is_empty(),
            "the fixture's stylesheet did not parse: {diagnostics:?}"
        );
        let pass = engine.restyle(&mut document, None);
        assert!(pass.styled > 0, "the fixture styled nothing");
        Self {
            document,
            root,
            engine,
            sheets: vec![handle],
        }
    }

    /// Changes the document through the mutation protocol and restyles it against the same sheets.
    ///
    /// Through the protocol, because a change made behind it is a change no traversal is ever told
    /// about — the restyle would run and find nothing to do.
    pub(crate) fn edit_and_restyle<R>(
        &mut self,
        body: impl FnOnce(&mut zgui_dom::Edit<'_>) -> R,
    ) -> R {
        let filter = self.engine.filter();
        let result = self
            .document
            .edit(&filter, body)
            .expect("the document is not poisoned");
        self.engine.restyle(&mut self.document, None);
        result
    }

    /// The box tree for this document.
    pub(crate) fn box_tree(&self) -> LayoutStore {
        let mut store = LayoutStore::new(self.document.store().document());
        zgui_layout::boxtree::build(&mut store, &self.document);
        store
    }
}

/// A replaced-content source reporting one natural size for everything.
#[derive(Debug)]
struct FixedIntrinsic {
    /// The size every replaced node reports.
    size: zgui_geom::Size<CssPx, zgui_geom::Css>,
}

impl ReplacedContent for FixedIntrinsic {
    fn intrinsic(&self, _id: ReplacedId) -> Intrinsic {
        Intrinsic {
            size: Some(self.size),
            ratio: (self.size.height.0 != 0.0).then(|| self.size.width.0 / self.size.height.0),
            baseline: None,
        }
    }
}

/// Appends one fixture element and everything below it.
fn append(document: &mut Document, parent: NodeIndex, element: &Element) -> NodeIndex {
    let index = document.append(parent, NodeKind::Element, ElementName::new(element.name));
    if !element.classes.is_empty() {
        let classes: Vec<ClassName> = element
            .classes
            .iter()
            .copied()
            .map(ClassName::new)
            .collect();
        document.set_classes(index, &classes);
    }
    if let Some((width, height)) = element.replaced {
        document.set_flags(index, NodeFlags::IS_REPLACED);
        let _ = (width, height);
    }
    if let Some(text) = element.text {
        let text_node = document.append(index, NodeKind::Text, ElementName::new("#text"));
        zgui_dom::text::node::set_text(document.store_mut(), text_node, text);
    }
    for child in &element.children {
        append(document, index, child);
    }
    index
}

/// One measurement the engine asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ask {
    /// The width the box was told it had, if it was told one.
    pub(crate) known_width: Option<f32>,
    /// The space it was told was available on the inline axis.
    pub(crate) available_width: taffy::AvailableSpace,
    /// Whether the answer was going to be kept.
    pub(crate) final_pass: bool,
}

impl Ask {
    /// The definite width this ask offered, if it offered one.
    pub(crate) fn definite_width(&self) -> Option<f32> {
        match self.available_width {
            taffy::AvailableSpace::Definite(width) => Some(width),
            _ => None,
        }
    }
}

/// A measurer for content the layout engine does not lay out, which records every question.
///
/// It answers a fixed natural size, which is what an image is: the box around it decides how much
/// of that size survives, and a measurer that resized itself to the question would hide that.
#[derive(Debug, Default)]
pub(crate) struct Images {
    /// Every ask, in the order they arrived.
    pub(crate) asks: Vec<Ask>,
    /// The natural size reported for every replaced box.
    pub(crate) natural: (f32, f32),
}

impl Images {
    /// A measurer reporting one natural size for everything replaced.
    pub(crate) fn new(width: f32, height: f32) -> Self {
        Self {
            asks: Vec::new(),
            natural: (width, height),
        }
    }
}

impl MeasureContent for Images {
    fn measure(&mut self, request: MeasureRequest<'_>) -> Measured {
        self.asks.push(Ask {
            known_width: request.known.width,
            available_width: request.available.width,
            final_pass: request.final_pass,
        });
        let (width, height) = self.natural;
        Measured::sized(request.known.width.unwrap_or(width), height)
    }

    fn shape(
        &mut self,
        _content: &zgui_text::ParagraphContent<'_>,
    ) -> zgui_layout::measure::ShapedSummary {
        zgui_layout::measure::ShapedSummary::default()
    }

    fn break_lines(
        &mut self,
        _key: zgui_text::ParagraphKey,
        _request: &zgui_text::BreakRequest<'_>,
    ) -> zgui_text::BrokenParagraph {
        zgui_text::BrokenParagraph::default()
    }

    fn strut(&mut self, _style: &zgui_text_style::TextStyle) -> zgui_text::StrutMetrics {
        zgui_text::StrutMetrics::default()
    }

    fn paint_slot(&mut self, _paint: &zgui_text_style::TextPaint) -> zgui_text::Brush {
        zgui_scene::PaintSlot(0)
    }
}

/// The measurer every layout test uses: a deterministic shaper, and images of a fixed size.
pub(crate) type Content = Paragraphs<mono::MonoShaper, Images>;

/// A measurer with the deterministic shaper behind it.
pub(crate) fn measurer() -> Content {
    Paragraphs::with_replaced(mono::MonoShaper::default(), Images::new(0.0, 0.0))
}

/// The same, with replaced content of one natural size.
pub(crate) fn measurer_with_images(width: f32, height: f32) -> Content {
    Paragraphs::with_replaced(mono::MonoShaper::default(), Images::new(width, height))
}

/// Everything a frame carries around a layout store: the side tables whose identifiers outlive the
/// frame, the scroll offsets, the hit index and the damage the pass accumulated.
pub(crate) struct Frame {
    /// Every clip chain in the document.
    pub(crate) clips: zgui_scene::ClipTable,
    /// Every transform in it.
    pub(crate) spatial: zgui_scene::SpatialTree,
    /// Where each scroll container is scrolled to.
    pub(crate) scroll: zgui_layout::scroll_region::ScrollOffsets,
    /// What is under a point.
    pub(crate) hit: zgui_layout::HitIndex,
    /// What the last pass said must be redrawn.
    pub(crate) damage: zgui_bits::DamageSet,
}

impl Frame {
    /// A frame that has drawn nothing yet.
    pub(crate) fn new() -> Self {
        Self {
            clips: zgui_scene::ClipTable::rooted(),
            spatial: zgui_scene::SpatialTree::with_viewport(),
            scroll: zgui_layout::scroll_region::ScrollOffsets::new(),
            hit: zgui_layout::HitIndex::new(),
            damage: zgui_bits::DamageSet::new(),
        }
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

/// Lays a store out at one viewport size and composes its fragments.
///
/// Both halves, because the device-pixel rounding a resolved layout is read at happens in the
/// fragment pass: a caller that laid out and stopped would be reading unrounded numbers.
pub(crate) fn lay_out(
    store: &mut LayoutStore,
    content: &mut Content,
    width: f32,
    height: f32,
) -> Frame {
    let mut frame = Frame::new();
    relayout(&mut frame, store, content, width, height);
    frame
}

/// Runs the layout pass over a store and stops there.
///
/// The layout pass on its own: no fragments, no hit index and no invariant check. What laying out
/// costs and what composing a frame around it costs are two different numbers, and a measurement
/// that wants one of them has to be able to take it without the other.
pub(crate) fn lay_out_only(
    store: &mut LayoutStore,
    content: &mut Content,
    width: f32,
    height: f32,
) {
    lay_out_at_scale(store, content, width, height, DeviceStyle::default().scale);
}

/// The same, on a display of a given device-pixel ratio.
///
/// Every length the layout algorithms work in is in device pixels, so the scale is not a detail of
/// the output: it is an input that every measurement was taken against.
pub(crate) fn lay_out_at_scale(
    store: &mut LayoutStore,
    content: &mut Content,
    width: f32,
    height: f32,
    scale: f32,
) {
    let device = DeviceStyle {
        scale,
        ..DeviceStyle::default()
    };
    let mut tree = LayoutTree::new(store, content, device);
    assert!(
        tree.layout_root(taffy::Size { width, height }),
        "the fixture generated no root box"
    );
}

/// The same, against tables and an index that already exist, which is what a second frame does.
pub(crate) fn relayout(
    frame: &mut Frame,
    store: &mut LayoutStore,
    content: &mut Content,
    width: f32,
    height: f32,
) {
    lay_out_only(store, content, width, height);
    let root = store.root().expect("a root box");
    fragments(
        frame,
        store,
        root,
        &mut zgui_layout::fragment::diff::Everything,
    );
}

/// Composes fragments for one subtree, then checks that the three levels still agree.
///
/// The check runs here rather than in a suite of its own so that every test in the crate makes it:
/// an invariant is worth what the test that broke it reports, and a violation found by a dedicated
/// test names nothing.
pub(crate) fn fragments(
    frame: &mut Frame,
    store: &mut LayoutStore,
    root: zgui_layout::BoxKey,
    dirty: &mut impl zgui_layout::fragment::diff::FrameDirty,
) {
    let mut tables = zgui_layout::fragment::build::Tables {
        clips: &mut frame.clips,
        spatial: &mut frame.spatial,
        device: DeviceStyle::default(),
        scroll: &frame.scroll,
        placements: &[],
    };
    zgui_layout::fragment::diff::rebuild(
        store,
        &mut frame.hit,
        &mut tables,
        dirty,
        root,
        &mut frame.damage,
    );
    let violations = zgui_layout::invariants::check(store, &frame.hit);
    assert!(violations.is_empty(), "{violations:?}");
}
