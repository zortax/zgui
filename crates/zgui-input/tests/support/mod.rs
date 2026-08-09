//! A styled, laid-out document with a hit index over it, and a router aimed at it.
//!
//! Everything an input test asserts is a claim about a real pipeline: a real cascade decided what
//! matches, a real layout decided where the boxes are, and a real fragment pass wrote the entries
//! the hit test descends. A stand-in for any of the three would make a budget a measurement of the
//! stand-in.

// Each test target uses a different part of this.
#![allow(dead_code, unreachable_pub)]

use std::sync::Arc;

use zgui_dom::{Document, EverythingMatters, NodeIndex, NodeKey, NodeKind};
use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Scale};
use zgui_input::{Router, World};
use zgui_interned::{AttrName, ClassName, ElementName};
use zgui_layout::HitIndex;
use zgui_layout::style::DeviceStyle;
use zgui_layout::text::Paragraphs;
use zgui_layout::tree::LayoutTree;
use zgui_layout::tree::store::LayoutStore;
use zgui_style::{SheetOrigin, SheetSource, StyleEngine, Viewport};
use zgui_testkit_scene::{FixedMetrics, MonoShaper};
use zgui_vocab::SharedString;

/// One element of a fixture tree.
pub struct Element {
    /// The element's name.
    pub name: &'static str,
    /// Its classes.
    pub classes: Vec<&'static str>,
    /// Its attributes.
    pub attrs: Vec<(&'static str, &'static str)>,
    /// Its children.
    pub children: Vec<Element>,
}

impl Element {
    /// An element with nothing but a name.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            classes: Vec::new(),
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }

    /// The same element, with one class.
    pub fn class(mut self, class: &'static str) -> Self {
        self.classes.push(class);
        self
    }

    /// The same element, with one attribute.
    pub fn attr(mut self, name: &'static str, value: &'static str) -> Self {
        self.attrs.push((name, value));
        self
    }

    /// The same element, with children.
    pub fn children(mut self, children: Vec<Element>) -> Self {
        self.children = children;
        self
    }
}

/// The measurer: a shaper with a fixed face, and nothing replaced.
type Content = Paragraphs<MonoShaper, NoImages>;

/// Replaced content that answers nothing, because these fixtures have none.
#[derive(Default)]
pub struct NoImages;

/// This measurer shapes nothing, so it has no cluster to report.
impl zgui_text::ShapedClusters for NoImages {
    fn visit_clusters(
        &self,
        _paragraph: zgui_text::ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(zgui_text::ClusterRun<'_>),
    ) {
    }
}

impl zgui_layout::measure::MeasureContent for NoImages {
    fn measure(
        &mut self,
        request: zgui_layout::measure::MeasureRequest<'_>,
    ) -> zgui_layout::measure::Measured {
        zgui_layout::measure::Measured::sized(request.known.width.unwrap_or(0.0), 0.0)
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

/// A whole frame: the document, its geometry, and the tables a hit test reads.
pub struct Fixture {
    /// The document.
    pub document: Document,
    /// The root element.
    pub root: NodeIndex,
    /// The box and fragment trees.
    pub layout: LayoutStore,
    /// What is under a point.
    pub hit: HitIndex,
    /// The clip chains the fragments were measured against.
    pub clips: zgui_scene::ClipTable,
    /// The coordinate systems they were measured in.
    pub spatial: zgui_scene::SpatialTree,
    /// What the fragment pass said must be redrawn.
    pub damage: zgui_bits::DamageSet,
    /// The engine, held because it owns the rule set the document was styled against.
    pub engine: StyleEngine,
    /// The sheet handles, held because dropping one removes its sheet.
    sheets: Vec<zgui_style::SheetHandle>,
    /// The viewport everything was laid out in.
    viewport: (f32, f32),
}

impl Fixture {
    /// Builds a document from `tree`, cascades `css` over it, lays it out and indexes it.
    pub fn new(tree: Element, css: &str) -> Self {
        let mut document = Document::new();
        let document_index = document.document_index();
        let root = build(&document, document_index, &tree);
        let mut engine = StyleEngine::new(
            &document,
            Arc::new(FixedMetrics::new()),
            Viewport::new(CssPx(800.0), CssPx(600.0)),
        );
        let (handle, diagnostics) =
            engine.add_sheet(&document, SheetOrigin::Author, SheetSource::Text(css));
        assert!(
            diagnostics.is_empty(),
            "the fixture's stylesheet did not parse: {diagnostics:?}"
        );
        let pass = engine.restyle(&mut document, None);
        assert!(pass.styled > 0, "the fixture styled nothing");

        let empty = LayoutStore::new(document.store().document());
        let mut fixture = Self {
            document,
            root,
            layout: empty,
            hit: HitIndex::new(),
            clips: zgui_scene::ClipTable::rooted(),
            spatial: zgui_scene::SpatialTree::with_viewport(),
            damage: zgui_bits::DamageSet::new(),
            engine,
            sheets: vec![handle],
            viewport: (800.0, 600.0),
        };
        fixture.lay_out();
        fixture.settle();
        fixture
    }

    /// Retires every obligation the document is still carrying.
    ///
    /// A frame does this by running: paint consumes what it repaints, the accessibility pass
    /// consumes what it projects, and each retires the record of which children owed it. These
    /// tests run the style and layout halves and stop, so without this the document is left owing
    /// work to stages that are not here — and the *next* walk over it prices in every element the
    /// first pass touched, which would make a hover budget a measurement of the missing stages.
    pub fn settle(&mut self) {
        self.document.retire(zgui_bits::Dirty::all());
    }

    /// Builds the box tree, lays it out, composes fragments and rebuilds the index.
    pub fn lay_out(&mut self) {
        let mut layout = LayoutStore::new(self.document.store().document());
        zgui_layout::boxtree::build(&mut layout, &self.document);
        let mut content = Paragraphs::with_replaced(MonoShaper::default(), NoImages);
        let laid = LayoutTree::new(&mut layout, &mut content, DeviceStyle::default())
            .layout_viewport(self.viewport.0, self.viewport.1);
        assert!(laid, "the fixture generated no root box");
        self.layout = layout;

        let root = self.layout.root().expect("a root box");
        let scroll = zgui_layout::scroll_region::ScrollOffsets::new();
        let mut tables = zgui_layout::fragment::build::Tables {
            clips: &mut self.clips,
            spatial: &mut self.spatial,
            device: DeviceStyle::default(),
            scroll: &scroll,
            placements: &[],
        };
        zgui_layout::fragment::diff::rebuild(
            &mut self.layout,
            &mut self.hit,
            &mut tables,
            &mut zgui_layout::fragment::diff::Everything,
            root,
            &mut self.damage,
        );
    }

    /// The frame an event is routed against.
    ///
    /// The filter is borrowed rather than owned, so a caller keeps it alive for as long as the
    /// world: it is the rule set's own view of which changes can matter, and building one per
    /// event would rebuild that view per event.
    pub fn world<'a>(&'a self, filter: &'a zgui_style::StyleFilterView<'a>) -> World<'a> {
        World {
            document: &self.document,
            layout: &self.layout,
            hit: &self.hit,
            clips: &self.clips,
            spatial: &self.spatial,
            scale: Scale::<Css, Device>::new(1.0),
            filter,
        }
    }

    /// The rule set's view of which changes can affect a computed style.
    pub fn filter(&self) -> zgui_style::StyleFilterView<'_> {
        self.engine.filter()
    }

    /// Runs a restyle and reports how many elements it styled.
    pub fn restyle(&mut self) -> usize {
        self.engine.restyle(&mut self.document, None).styled
    }

    /// The first element named `name`, in document order.
    pub fn find(&self, name: &str) -> NodeIndex {
        find(&self.document, self.root, name)
            .unwrap_or_else(|| panic!("no `{name}` in the fixture"))
    }

    /// That element's generation-checked name.
    pub fn key(&self, name: &str) -> NodeKey {
        self.document.store().key_of(self.find(name))
    }

    /// The centre of one element's first box, in device pixels.
    pub fn centre_of(&self, name: &str) -> Point<DevicePx, Device> {
        let key = self.key(name);
        let box_ = *self
            .layout
            .boxes_of(key)
            .first()
            .unwrap_or_else(|| panic!("`{name}` generated no box"));
        let fragment = *self
            .layout
            .fragments_of_box(box_)
            .first()
            .unwrap_or_else(|| panic!("`{name}` generated no fragment"));
        let rect = self
            .layout
            .fragment(fragment)
            .expect("a live fragment")
            .border_box;
        Point::new(
            DevicePx(rect.origin.x.0 + rect.size.width.0 / 2.0),
            DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
        )
    }

    /// How many elements are between the root and `name`, inclusive of `name`.
    pub fn depth_of(&self, name: &str) -> usize {
        let mut depth = 0;
        let mut index = Some(self.find(name));
        while let Some(current) = index {
            if self.document.store().core(current).kind() == NodeKind::Element {
                depth += 1;
            }
            index = self.document.store().core(current).parent();
        }
        depth
    }
}

/// A router with a fixture behind it, so a test can write one line per gesture.
pub struct Session {
    /// The document and its geometry.
    pub fixture: Fixture,
    /// The input system's own state.
    pub router: Router,
}

impl Session {
    /// Opens one over `fixture`.
    pub fn new(fixture: Fixture) -> Self {
        Self {
            fixture,
            router: Router::new(),
        }
    }

    /// Moves the pointer to the centre of `name`, and reports what was routed.
    pub fn hover(&mut self, name: &str) -> Vec<NodeKey> {
        let point = self.fixture.centre_of(name);
        self.pointer_at(point, zgui_vocab::PointerAction::Moved)
    }

    /// Presses at a point, and reports the resolved order.
    pub fn press(&mut self, point: Point<DevicePx, Device>) -> Vec<zgui_input::Step> {
        self.press_with_default(point).0
    }

    /// The same, with what the framework would do about it on its own account.
    pub fn press_with_default(
        &mut self,
        point: Point<DevicePx, Device>,
    ) -> (Vec<zgui_input::Step>, Option<zgui_input::FrameworkDefault>) {
        self.route(point, zgui_vocab::PointerAction::Pressed, |routed| {
            (routed.steps.to_vec(), routed.default)
        })
    }

    /// What the framework would do about one pointer action at a point.
    pub fn default_at(
        &mut self,
        point: Point<DevicePx, Device>,
        action: zgui_vocab::PointerAction,
    ) -> Option<zgui_input::FrameworkDefault> {
        self.route(point, action, |routed| routed.default)
    }

    /// Routes one pointer action at a point and takes what `read` wants out of the answer.
    pub fn route<R>(
        &mut self,
        point: Point<DevicePx, Device>,
        action: zgui_vocab::PointerAction,
        read: impl FnOnce(&zgui_input::Routed<'_>) -> R,
    ) -> R {
        let event = zgui_vocab::PointerEvent::mouse(Point::new(CssPx(point.x.0), CssPx(point.y.0)));
        let filter = self.fixture.filter();
        let world = self.fixture.world(&filter);
        let routed = self.router.pointer(
            &world,
            action,
            &event,
            zgui_vocab::Modifiers::NONE,
            zgui_vocab::Timestamp::ORIGIN,
        );
        read(&routed)
    }

    /// Moves the pointer to a point, and reports the path the event travelled.
    pub fn pointer_at(
        &mut self,
        point: Point<DevicePx, Device>,
        action: zgui_vocab::PointerAction,
    ) -> Vec<NodeKey> {
        let event = zgui_vocab::PointerEvent::mouse(Point::new(CssPx(point.x.0), CssPx(point.y.0)));
        let filter = self.fixture.filter();
        let world = self.fixture.world(&filter);
        let routed = self.router.pointer(
            &world,
            action,
            &event,
            zgui_vocab::Modifiers::NONE,
            zgui_vocab::Timestamp::ORIGIN,
        );
        routed.chain.path().to_vec()
    }
}

/// Builds one fixture element and everything below it, through the document's own batch.
///
/// Through the batch even though nothing is watching yet, because the batch is where a change's
/// obligations are discharged and a fixture built around it would be a fixture whose costs are not
/// the costs of the thing being measured.
fn build(document: &Document, parent: NodeIndex, element: &Element) -> NodeIndex {
    let index = document
        .edit(&EverythingMatters, |edit| {
            let index = edit.create_element(ElementName::new(element.name));
            edit.insert_before(parent, index, None);
            if !element.classes.is_empty() {
                let classes: Vec<ClassName> = element
                    .classes
                    .iter()
                    .copied()
                    .map(ClassName::new)
                    .collect();
                edit.set_classes(index, &classes);
            }
            for (name, value) in &element.attrs {
                edit.set_attribute(index, AttrName::new(name), Some(SharedString::from(*value)));
            }
            index
        })
        .expect("not poisoned");
    for child in &element.children {
        build(document, index, child);
    }
    index
}

/// The first element named `name` at or below `from`.
fn find(document: &Document, from: NodeIndex, name: &str) -> Option<NodeIndex> {
    let record = document.store().core(from);
    if record.kind() == NodeKind::Element && record.local_name().as_str() == name {
        return Some(from);
    }
    let mut child = record.first_child();
    while let Some(current) = child {
        if let Some(found) = find(document, current, name) {
            return Some(found);
        }
        child = document.store().core(current).next_sibling();
    }
    None
}
