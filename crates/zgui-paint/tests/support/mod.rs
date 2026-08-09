//! A styled document, laid out, with fragments composed and a painter over it.
//!
//! Everything here exists so that a paint test measures the paint stage: a real cascade over a real
//! document, real fragments from the real fragment diff, and a measurer with no text engine behind
//! it — because a paint test that depended on a font engine would be measuring the font engine.

// The harness is compiled into several test targets and each uses a different part of it.
#![allow(dead_code, unreachable_pub)]

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_dom::host::{Intrinsic, ReplacedContent, ReplacedId};
use zgui_dom::node::flags::NodeFlags;
use zgui_dom::{Document, NodeIndex, NodeKind};
use zgui_geom::{CssPx, Device, Size};
use zgui_interned::{ClassName, ElementName};
use zgui_layout::measure::{MeasureContent, MeasureRequest, Measured, ShapedSummary};
use zgui_layout::style::DeviceStyle;
use zgui_layout::tree::LayoutTree;
use zgui_layout::tree::store::LayoutStore;
use zgui_layout::{BoxKey, FragKey};
use zgui_paint::{PaintInput, Painter};
use zgui_scene::Scene;
use zgui_style::{SheetOrigin, SheetSource, StyleEngine, Viewport};
use zgui_text::{BreakRequest, BrokenParagraph, ParagraphContent, ParagraphKey, StrutMetrics};
use zgui_text_style::TextPaint;
use zgui_text_style::TextStyle;

/// One element of a fixture tree.
pub(crate) struct Element {
    /// The element's name, which selectors match on.
    pub(crate) name: &'static str,
    /// Its classes.
    pub(crate) classes: &'static [&'static str],
    /// Whether its content comes from outside the document.
    pub(crate) replaced: bool,
    /// The outlines it draws, as path notation, and the space they are written in.
    pub(crate) drawing: Option<(&'static str, Option<&'static str>)>,
    /// The retained canvas it shows, as the packed token-and-revision reference.
    pub(crate) canvas: Option<i64>,
    /// Its children.
    pub(crate) children: Vec<Element>,
}

impl Element {
    /// An element with nothing but a name.
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            classes: &[],
            replaced: false,
            drawing: None,
            canvas: None,
            children: Vec::new(),
        }
    }

    /// The same element, with classes.
    pub(crate) fn classes(mut self, classes: &'static [&'static str]) -> Self {
        self.classes = classes;
        self
    }

    /// The same element, replaced by content from outside the document.
    pub(crate) fn image(mut self) -> Self {
        self.replaced = true;
        self
    }

    /// The same element, drawing the given outlines in a space of its own.
    pub(crate) fn drawing(mut self, paths: &'static str, view_box: Option<&'static str>) -> Self {
        self.drawing = Some((paths, view_box));
        self
    }

    /// The same element, showing a retained canvas scene.
    pub(crate) fn canvas(mut self, handle: &zgui_canvas::SceneHandle) -> Self {
        self.canvas = Some(zgui_vocab::prop::drawing::canvas_value(
            handle.token().0,
            handle.revision(),
        ));
        self
    }

    /// The same element, with children.
    pub(crate) fn children(mut self, children: Vec<Element>) -> Self {
        self.children = children;
        self
    }
}

/// A styled document with a box tree, fragments, a scene and a painter over it.
pub(crate) struct Harness {
    /// The document.
    pub(crate) document: Document,
    /// The root element.
    pub(crate) root: NodeIndex,
    /// The boxes, their layout results and their fragments.
    pub(crate) store: LayoutStore,
    /// The display list.
    pub(crate) scene: Scene,
    /// The paint stage's state between frames.
    pub(crate) painter: Painter,
    /// What the last fragment pass said must be redrawn.
    pub(crate) damage: DamageSet,
    /// The surface extent.
    pub(crate) viewport: Size<i32, Device>,
    /// How many device pixels one CSS pixel is.
    pub(crate) scale: f32,
    /// Where each scroll container is scrolled to.
    scroll: zgui_layout::scroll_region::ScrollOffsets,
    /// What is under a point.
    pub(crate) hit: zgui_layout::HitIndex,
    /// The measurer, which has no text engine behind it.
    content: Measurer,
    /// The engine, held because it owns the rule set the document was styled against.
    engine: StyleEngine,
    /// The installed sheets' handles, held because dropping one removes its sheet.
    sheets: Vec<zgui_style::SheetHandle>,
}

impl Harness {
    /// Builds a document from `tree`, cascades `css` over it, lays it out and composes fragments.
    pub(crate) fn new(tree: Element, css: &str) -> Self {
        Self::sized(tree, css, 400.0, 400.0)
    }

    /// The same, over a surface of the given extent.
    pub(crate) fn sized(tree: Element, css: &str, width: f32, height: f32) -> Self {
        let mut document = Document::new();
        document.install_replaced_content(Arc::new(UnitSquare));
        let document_index = document.document_index();
        let root = append(&mut document, document_index, &tree);
        let mut engine = StyleEngine::new(
            &document,
            Arc::new(zgui_text::FixedMetrics::new()),
            Viewport::new(CssPx(width), CssPx(height)),
        );
        let (handle, diagnostics) =
            engine.add_sheet(&document, SheetOrigin::Author, SheetSource::Text(css));
        assert!(
            diagnostics.is_empty(),
            "the fixture's stylesheet did not parse: {diagnostics:?}"
        );
        let pass = engine.restyle(&mut document, None);
        assert!(pass.styled > 0, "the fixture styled nothing");

        let mut store = LayoutStore::new(document.store().document());
        zgui_layout::boxtree::build(&mut store, &document);
        let mut harness = Self {
            document,
            root,
            store,
            scene: Scene::new(),
            painter: Painter::new(),
            damage: DamageSet::new(),
            viewport: Size::new(width as i32, height as i32),
            scale: 1.0,
            scroll: zgui_layout::scroll_region::ScrollOffsets::new(),
            hit: zgui_layout::HitIndex::new(),
            content: Measurer,
            engine,
            sheets: vec![handle],
        };
        harness.compose(width, height);
        harness
    }

    /// Lays the document out and composes its fragments, accumulating damage as it goes.
    pub(crate) fn compose(&mut self, width: f32, height: f32) {
        let mut tree = LayoutTree::new(&mut self.store, &mut self.content, DeviceStyle::default());
        assert!(
            tree.layout_viewport(width, height),
            "the fixture generated no root box"
        );
        let root = self.store.root().expect("a root box");
        // The clip chains and transforms a fragment names are the *scene's* tables, not a second
        // set: a fragment carrying an index into one table and a primitive resolving it in another
        // is how a replayed range comes to be drawn with somebody else's clip.
        let mut tables = zgui_layout::fragment::build::Tables {
            clips: &mut self.scene.clips,
            spatial: &mut self.scene.spatial,
            device: DeviceStyle::default(),
            scroll: &self.scroll,
            placements: &[],
        };
        zgui_layout::fragment::diff::rebuild(
            &mut self.store,
            &mut self.hit,
            &mut tables,
            &mut zgui_layout::fragment::diff::Everything,
            root,
            &mut self.damage,
        );
    }

    /// The same, reading what is dirty out of the document instead of assuming everything is.
    ///
    /// [`compose`](Self::compose) answers "everything is dirty", which is the honest answer for a
    /// first build and a useless one for a damage assertion: a pass told that every box owes every
    /// phase re-examines every fragment and produces damage whatever the change was, so a set that
    /// covers an element proves nothing about the invalidation that was supposed to reach it. This
    /// answers from the document's own marks, exactly as a frame does, so a change that marked
    /// nothing composes to no damage at all.
    pub(crate) fn compose_from_marks(&mut self, width: f32, height: f32) {
        let mut tree = LayoutTree::new(&mut self.store, &mut self.content, DeviceStyle::default());
        assert!(
            tree.layout_viewport(width, height),
            "the fixture generated no root box"
        );
        let root = self.store.root().expect("a root box");
        let mut tables = zgui_layout::fragment::build::Tables {
            clips: &mut self.scene.clips,
            spatial: &mut self.scene.spatial,
            device: DeviceStyle::default(),
            scroll: &self.scroll,
            placements: &[],
        };
        let mut marks =
            zgui_layout::fragment::diff::DocumentMarks::for_document(&mut self.document);
        zgui_layout::fragment::diff::rebuild(
            &mut self.store,
            &mut self.hit,
            &mut tables,
            &mut marks,
            root,
            &mut self.damage,
        );
    }

    /// Rebuilds the box tree and recomposes, which is what a structural change costs.
    pub(crate) fn rebuild(&mut self, width: f32, height: f32) {
        self.store = LayoutStore::new(self.document.store().document());
        zgui_layout::boxtree::build(&mut self.store, &self.document);
        self.compose(width, height);
    }

    /// Changes the document through the mutation protocol and restyles it against the same sheets.
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

    /// Grows the damage over the read-extent registry, exactly as a frame does before emitting.
    pub(crate) fn expand(&mut self) -> zgui_paint::Expansion {
        zgui_paint::expand(&self.store, &mut self.damage, self.viewport, self.scale)
    }

    /// Emits a frame against the damage the fragment pass accumulated.
    pub(crate) fn paint(&mut self) -> zgui_paint::PaintReport {
        self.scene.begin_frame(self.viewport);
        // Recorded in every profile. The tests below compare what was emitted against what the
        // damage reaches, so a recording that only happened in one profile would leave the other
        // asserting nothing at all.
        let mut input = PaintInput::new(&self.store, &self.damage);
        input.record_emitted = true;
        let report = self.painter.emit(&input, &mut self.scene);
        self.scene.finish(&self.damage);
        report
    }

    /// Emits a frame drawing its glyphs and its images through `content`.
    ///
    /// The whole surface is damaged, because a test about *what* content reaches the display list
    /// is not also a test about which rectangles were reached.
    pub(crate) fn paint_content(
        &mut self,
        content: &mut zgui_paint::ContentCache,
        raster: &dyn zgui_text::GlyphRaster,
    ) -> zgui_paint::PaintReport {
        self.damage = DamageSet::full();
        self.scene.begin_frame(self.viewport);
        content.begin_frame();
        let report = {
            let frame = content.frame(&self.store, &NoGlyphs, raster);
            let input = PaintInput {
                glyphs: &frame,
                replaced: &frame,
                ..PaintInput::new(&self.store, &self.damage)
            };
            self.painter.emit(&input, &mut self.scene)
        };
        self.scene.finish(&self.damage);
        report
    }

    /// Emits a frame drawing the outlines its elements carry, with the whole surface damaged.
    ///
    /// The drawings are read from the document through the same cache a window installs, so this
    /// exercises the real source rather than a fixture standing in for one.
    pub(crate) fn paint_vectors(
        &mut self,
        vectors: &zgui_paint::VectorCache,
    ) -> zgui_paint::PaintReport {
        self.damage = DamageSet::full();
        self.scene.begin_frame(self.viewport);
        let report = {
            let frame = vectors.frame(&self.document);
            let mut input = PaintInput {
                vectors: &frame,
                scale: self.scale,
                ..PaintInput::new(&self.store, &self.damage)
            };
            input.record_emitted = true;
            self.painter.emit(&input, &mut self.scene)
        };
        self.scene.finish(&self.damage);
        report
    }

    /// Emits drawings through the real shared atlas, enabling the small-vector mask route.
    pub(crate) fn paint_cached_vectors(
        &mut self,
        vectors: &zgui_paint::VectorCache,
        content: &mut zgui_paint::ContentCache,
        raster: &dyn zgui_text::GlyphRaster,
    ) -> zgui_paint::PaintReport {
        self.damage = DamageSet::full();
        self.scene.begin_frame(self.viewport);
        content.begin_frame();
        let report = {
            let drawings = vectors.frame(&self.document);
            let frame = content.frame(&self.store, &NoGlyphs, raster);
            let mut input = PaintInput {
                vectors: &drawings,
                vector_masks: &frame,
                resources: &frame,
                scale: self.scale,
                ..PaintInput::new(&self.store, &self.damage)
            };
            input.record_emitted = true;
            self.painter.emit(&input, &mut self.scene)
        };
        self.scene.finish(&self.damage);
        report
    }

    /// The document, for a test that changes a property a view would have written.
    pub(crate) fn document(&self) -> &Document {
        &self.document
    }

    /// The fragment tree.
    pub(crate) fn store(&self) -> &LayoutStore {
        &self.store
    }

    /// The scene the last paint produced.
    pub(crate) fn scene(&self) -> &Scene {
        &self.scene
    }

    /// The replaced identifier of a named element.
    pub(crate) fn replaced_id(&self, name: &str) -> ReplacedId {
        ReplacedId::new(self.document.store().key_of(self.element(name)))
    }

    /// The replaced identifiers of every element of one name, in document order.
    pub(crate) fn replaced_ids(&self, name: &str) -> Vec<ReplacedId> {
        let mut out = Vec::new();
        let mut stack = vec![self.root];
        while let Some(index) = stack.pop() {
            let core = self.document.store().core(index);
            if core.local_name().as_str() == name {
                out.push(ReplacedId::new(self.document.store().key_of(index)));
            }
            let mut children = Vec::new();
            let mut child = core.first_child();
            while let Some(index) = child {
                children.push(index);
                child = self.document.store().core(index).next_sibling();
            }
            stack.extend(children.into_iter().rev());
        }
        out
    }

    /// Emits a frame with the whole surface damaged.
    pub(crate) fn paint_everything(&mut self) -> zgui_paint::PaintReport {
        self.damage = DamageSet::full();
        self.paint()
    }

    /// Forgets the damage, which is what the start of a frame does.
    pub(crate) fn clear_damage(&mut self) {
        self.damage = DamageSet::new();
    }

    /// The box a named element generated.
    pub(crate) fn box_of(&self, name: &str) -> BoxKey {
        let mut stack = vec![self.root];
        while let Some(index) = stack.pop() {
            let core = self.document.store().core(index);
            if core.local_name().as_str() == name {
                let key = self.document.store().key_of(index);
                let boxes = self.store.boxes_of(key);
                assert!(!boxes.is_empty(), "`{name}` generated no box");
                return boxes[0];
            }
            let mut child = core.first_child();
            while let Some(index) = child {
                stack.push(index);
                child = self.document.store().core(index).next_sibling();
            }
        }
        panic!("no element named `{name}`");
    }

    /// The first fragment a named element's box produced.
    pub(crate) fn fragment_of(&self, name: &str) -> FragKey {
        *self
            .store
            .fragments_of_box(self.box_of(name))
            .first()
            .expect("every box produces its own piece")
    }

    /// The element with the given name, as a document index.
    pub(crate) fn element(&self, name: &str) -> NodeIndex {
        let mut stack = vec![self.root];
        while let Some(index) = stack.pop() {
            let core = self.document.store().core(index);
            if core.local_name().as_str() == name {
                return index;
            }
            let mut child = core.first_child();
            while let Some(index) = child {
                stack.push(index);
                child = self.document.store().core(index).next_sibling();
            }
        }
        panic!("no element named `{name}`");
    }
}

/// A replaced-content source reporting one natural size for everything.
#[derive(Debug)]
struct UnitSquare;

impl ReplacedContent for UnitSquare {
    fn intrinsic(&self, _id: ReplacedId) -> Intrinsic {
        Intrinsic {
            size: Some(zgui_geom::Size::new(CssPx(32.0), CssPx(32.0))),
            ratio: Some(1.0),
            baseline: None,
        }
    }
}

/// A measurer with no text engine behind it.
///
/// Replaced content measures to whatever it is given, and text measures to nothing — which is what
/// a paint test wants, because a paint test that shaped a paragraph would be measuring the shaper.
struct Measurer;

impl zgui_text::ShapedClusters for Measurer {
    fn visit_clusters(
        &self,
        _paragraph: ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(zgui_text::ClusterRun<'_>),
    ) {
    }
}

impl MeasureContent for Measurer {
    fn measure(&mut self, request: MeasureRequest<'_>) -> Measured {
        Measured::sized(request.known.width.unwrap_or(32.0), 32.0)
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

    fn paint_slot(&mut self, _paint: &TextPaint) -> zgui_text::Brush {
        zgui_scene::PaintSlot(0)
    }
}

/// A source of positioned glyphs with nothing in it.
///
/// The replaced half of the content cache is exercised on its own here: a fixture that had to hold
/// a shaped paragraph in order to draw a picture would be testing the text engine.
struct NoGlyphs;

impl zgui_text::ShapedGlyphs for NoGlyphs {
    fn visit_line(
        &self,
        _paragraph: ParagraphKey,
        _line: u16,
        _visit: &mut dyn FnMut(zgui_text::ShapedRun<'_>),
    ) {
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
    if element.replaced {
        document.set_flags(index, NodeFlags::IS_REPLACED);
    }
    if let Some((paths, view_box)) = element.drawing {
        document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                edit.set_property(
                    index,
                    zgui_vocab::PropKey::new(zgui_vocab::prop::drawing::PATHS),
                    Some(zgui_vocab::PropValue::from(paths)),
                );
                if let Some(view_box) = view_box {
                    edit.set_property(
                        index,
                        zgui_vocab::PropKey::new(zgui_vocab::prop::drawing::VIEW_BOX),
                        Some(zgui_vocab::PropValue::from(view_box)),
                    );
                }
            })
            .expect("the fixture document is not poisoned");
    }
    if let Some(reference) = element.canvas {
        document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                edit.set_property(
                    index,
                    zgui_vocab::PropKey::new(zgui_vocab::prop::drawing::CANVAS),
                    Some(zgui_vocab::PropValue::Integer(reference)),
                );
            })
            .expect("the fixture document is not poisoned");
    }
    for child in &element.children {
        append(document, index, child);
    }
    index
}
