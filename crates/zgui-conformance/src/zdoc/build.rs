//! Turning a converted test into a styled document, a box tree and a laid-out fragment tree.
//!
//! Everything a conformance run compares comes out of here, and everything here is deterministic:
//! the shaper has a fixed face and opens no font file, replaced content answers the natural size
//! the document declared, and the viewport is the one the document names. A run therefore measures
//! the layout engine and not the machine it ran on.

use std::collections::HashMap;
use std::sync::Arc;

use zgui_css::ComputedStyle;
use zgui_dom::host::{Intrinsic, ReplacedContent, ReplacedId};
use zgui_dom::node::flags::NodeFlags;
use zgui_dom::{Document, NodeIndex, NodeKey, NodeKind};
use zgui_geom::{Css, CssPx, Size};
use zgui_interned::{ClassName, ElementName};
use zgui_layout::BoxKey;
use zgui_layout::measure::{MeasureContent, MeasureRequest, Measured, ShapedSummary};
use zgui_layout::style::DeviceStyle;
use zgui_layout::text::Paragraphs;
use zgui_layout::tree::LayoutTree;
use zgui_layout::tree::store::LayoutStore;
use zgui_style::{SheetOrigin, SheetSource, StyleEngine, Viewport};
use zgui_testkit_scene::{FixedMetrics, MonoShaper};

use crate::zdoc::source::{Element, Zdoc};

/// The display defaults a converted document is laid out against.
///
/// A conformance suite is written in a markup language whose elements already have display types,
/// and those defaults are part of what every test in it assumes. The conversion keeps the element
/// names and this sheet supplies what the markup language would have: without it a converted
/// flexbox test would lay its items out as inline text and fail for a reason that has nothing to
/// do with flexbox.
pub const DISPLAY_DEFAULTS: &str = "\
root, div, p, section, article, main, header, footer, li, ul, ol { display: block }
span, a, em, strong, b, i, label { display: inline }
img, video, canvas, input, button { display: inline-block }
";

/// The natural size of each replaced element, by node.
type Natural = HashMap<NodeKey, Size<CssPx, Css>>;

/// A converted test, styled and laid out.
pub struct Laid {
    /// The document, held because the box tree names its nodes.
    pub document: Document,
    /// The box and fragment trees.
    pub store: LayoutStore,
    /// Where the fragment pass recorded its clips and transforms.
    pub tables: Tables,
    /// The engine, held because dropping it would drop the sheets the document was styled against.
    engine: StyleEngine,
    /// The sheet handles, held for the same reason.
    _sheets: Vec<zgui_style::SheetHandle>,
}

impl Laid {
    /// Every element's computed style, in tree order.
    pub fn styles(&self) -> Vec<ComputedStyle> {
        let mut out = Vec::new();
        walk_styles(self.document.document_node(), &mut out);
        out
    }

    /// The engine that styled the document, for a consumer that wants to ask it something.
    pub fn engine(&self) -> &StyleEngine {
        &self.engine
    }
}

/// The identifier tables one fragment pass filled in.
pub struct Tables {
    /// Every clip chain the pass recorded.
    pub clips: zgui_scene::ClipTable,
    /// Every coordinate system it named.
    pub spatial: zgui_scene::SpatialTree,
    /// What is under a point.
    pub hit: zgui_layout::HitIndex,
    /// What the pass said has to be redrawn.
    pub damage: zgui_bits::DamageSet,
}

impl Tables {
    /// Tables that have recorded nothing yet.
    fn new() -> Self {
        Self {
            clips: zgui_scene::ClipTable::rooted(),
            spatial: zgui_scene::SpatialTree::with_viewport(),
            hit: zgui_layout::HitIndex::new(),
            damage: zgui_bits::DamageSet::new(),
        }
    }
}

/// Styles and lays out one converted test.
///
/// # Panics
///
/// Panics when the document's own style sheet does not parse. A converted suite is machine-written,
/// so a sheet the engine rejected is a converter bug, and swallowing it would leave a test that
/// passes while applying an empty sheet — which is the exact failure a conformance harness exists
/// to make impossible.
pub fn lay_out(source: &Zdoc) -> Laid {
    zgui_css::enable_css_features();

    let mut document = Document::new();
    let mut natural = Natural::new();
    let document_index = document.document_index();
    append(&mut document, document_index, &source.root, &mut natural);
    document.install_replaced_content(Arc::new(Declared {
        sizes: natural.clone(),
    }));

    let mut engine = StyleEngine::new(
        &document,
        Arc::new(FixedMetrics::new()),
        Viewport::new(CssPx(source.viewport.0), CssPx(source.viewport.1)),
    );
    let mut sheets = Vec::new();
    for (origin, text) in [
        (SheetOrigin::UserAgent, DISPLAY_DEFAULTS),
        (SheetOrigin::Author, source.css.as_str()),
    ] {
        let (handle, diagnostics) = engine.add_sheet(&document, origin, SheetSource::Text(text));
        assert!(
            diagnostics.is_empty(),
            "the engine dropped part of a {origin:?} sheet: {diagnostics:?}",
        );
        sheets.push(handle);
    }
    let pass = engine.restyle(&mut document, None);
    assert!(pass.styled > 0, "the document styled nothing");

    let mut store = LayoutStore::new(document.store().document());
    zgui_layout::boxtree::build(&mut store, &document);

    let replaced = Replaced::over(&store, &natural);
    let mut content = Paragraphs::with_replaced(MonoShaper::default(), replaced);
    let laid = LayoutTree::new(&mut store, &mut content, DeviceStyle::default())
        .layout_viewport(source.viewport.0, source.viewport.1);
    assert!(laid, "the document generated no root box");

    let mut tables = Tables::new();
    fragments(&mut store, &mut tables);

    Laid {
        document,
        store,
        tables,
        engine,
        _sheets: sheets,
    }
}

/// Composes the fragment tree and checks that the three levels still agree.
fn fragments(store: &mut LayoutStore, tables: &mut Tables) {
    let root = store.root().expect("a root box");
    let scroll = zgui_layout::scroll_region::ScrollOffsets::new();
    let mut build = zgui_layout::fragment::build::Tables {
        clips: &mut tables.clips,
        spatial: &mut tables.spatial,
        device: DeviceStyle::default(),
        scroll: &scroll,
        placements: &[],
    };
    zgui_layout::fragment::diff::rebuild(
        store,
        &mut tables.hit,
        &mut build,
        &mut zgui_layout::fragment::diff::Everything,
        root,
        &mut tables.damage,
    );
    let violations = zgui_layout::invariants::check(store, &tables.hit);
    assert!(violations.is_empty(), "{violations:?}");
}

/// Collects one node's computed style and those of everything below it.
fn walk_styles(node: zgui_dom::Node<'_>, out: &mut Vec<ComputedStyle>) {
    if let Some(style) = node.primary_style() {
        out.push(style);
    }
    let mut child = node.first_child_node();
    while let Some(current) = child {
        walk_styles(current, out);
        child = current.next_sibling_node();
    }
}

/// Appends one element and everything below it.
fn append(document: &mut Document, parent: NodeIndex, element: &Element, natural: &mut Natural) {
    let index = document.append(parent, NodeKind::Element, ElementName::new(&element.name));
    if !element.classes.is_empty() {
        let classes: Vec<ClassName> = element
            .classes
            .iter()
            .map(|class| ClassName::new(class))
            .collect();
        document.set_classes(index, &classes);
    }
    if let Some((width, height)) = element.replaced {
        document.set_flags(index, NodeFlags::IS_REPLACED);
        natural.insert(
            document.node(index).key(),
            Size::new(CssPx(width), CssPx(height)),
        );
    }
    if let Some(text) = &element.text {
        let node = document.append(index, NodeKind::Text, ElementName::new("#text"));
        zgui_dom::text::node::set_text(document.store_mut(), node, text);
    }
    for child in &element.children {
        append(document, index, child, natural);
    }
}

/// The natural sizes the document declared, answered by node.
#[derive(Debug)]
struct Declared {
    /// One size per replaced node.
    sizes: Natural,
}

impl ReplacedContent for Declared {
    fn intrinsic(&self, id: ReplacedId) -> Intrinsic {
        let Some(size) = self.sizes.get(&id.node()).copied() else {
            return Intrinsic::default();
        };
        Intrinsic {
            size: Some(size),
            ratio: (size.height.0 != 0.0).then(|| size.width.0 / size.height.0),
            baseline: None,
        }
    }
}

/// Sizes replaced boxes at the natural size the document declared for them.
///
/// It answers a fixed size rather than resizing itself to the question, because a measurer that
/// returned the space it was offered would make every `auto`-sized image fit by construction and
/// hide the sizing rules a conformance suite is there to check. A box the document declared no
/// size for measures as empty, which is what a picture that has produced no frame is.
#[derive(Debug, Default)]
struct Replaced {
    /// The declared natural size of each replaced box.
    sizes: HashMap<BoxKey, (f32, f32)>,
}

impl Replaced {
    /// Reads the declared natural sizes onto the boxes the elements generated.
    ///
    /// Keyed by box rather than by element because that is what a measurement request names, and
    /// the element cannot be looked up from inside the request: the store is borrowed by the pass
    /// that is asking.
    fn over(store: &LayoutStore, natural: &Natural) -> Self {
        let mut sizes = HashMap::new();
        for key in store.keys() {
            let Some(source) = store.node(key).source else {
                continue;
            };
            if let Some(size) = natural.get(&source) {
                sizes.insert(key, (size.width.0, size.height.0));
            }
        }
        Self { sizes }
    }
}

impl MeasureContent for Replaced {
    fn measure(&mut self, request: MeasureRequest<'_>) -> Measured {
        let (width, height) = self.sizes.get(&request.box_).copied().unwrap_or((0.0, 0.0));
        Measured::sized(
            request.known.width.unwrap_or(width),
            request.known.height.unwrap_or(height),
        )
    }

    fn shape(&mut self, _content: &zgui_text::ParagraphContent<'_>) -> ShapedSummary {
        ShapedSummary::default()
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
