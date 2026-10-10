//! The drawings one window has already produced, kept between frames.

use core::cell::{Cell, RefCell};
use std::sync::{Arc, Weak};

use rustc_hash::FxHashMap;
use zgui_dom::{Document, NodeKey};
use zgui_scene::kurbo::Affine;
use zgui_vocab::SharedString;

use crate::content::vectors::{Drawing, Placement, VectorSource, parse};
use crate::emit::vector::fit;

/// What kind of text a drawing was read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Path notation, one outline per line.
    Notation,
    /// A vector document.
    Document,
}

impl Kind {
    /// The tag a revision hash starts the text with.
    fn tag(self) -> u32 {
        match self {
            Self::Notation => 3,
            Self::Document => 2,
        }
    }
}

/// One source text, read once for every element that draws it.
#[derive(Debug)]
struct Parsed {
    /// The text the shapes were read from.
    source: SharedString,
    /// What kind of text it is.
    kind: Kind,
    /// The shapes, in their own space.
    shapes: Arc<[zgui_svg::Shape]>,
    /// The read document, for a document; it says which view box the shapes are fitted through.
    read: Option<Arc<zgui_svg::Document>>,
}

impl Parsed {
    /// Whether this was read from `source` as `kind`.
    fn reads(&self, kind: Kind, source: &SharedString) -> bool {
        self.kind == kind && (self.source.ptr_eq(source) || self.source == *source)
    }
}

/// Where one node's drawing came from.
#[derive(Clone, Debug)]
enum Source {
    /// A source text, shared with every node that draws the same text.
    Parsed(Arc<Parsed>),
    /// A canvas scene, by token, by revision and by view.
    Canvas {
        /// The scene's token.
        token: u32,
        /// The scene's revision.
        revision: u32,
        /// The scene's view.
        view: u64,
    },
}

/// What was produced for one node, and what it was produced from.
#[derive(Clone, Debug)]
struct Entry {
    /// What the drawing was read from.
    source: Source,
    /// The matrix the shapes are fitted with, as its six coefficients — compared rather than the
    /// box and the view box separately, because two different boxes that fit to the same matrix
    /// produce the same picture and making a new drawing would throw away its placed cells for
    /// nothing.
    placed: [f64; 6],
    /// The result.
    drawing: Drawing,
}

/// One node's revision, and the text it was hashed from.
#[derive(Clone, Debug)]
struct Memo {
    /// The text, held so that its address names it for as long as the memo stands.
    source: SharedString,
    /// What kind of text it is.
    kind: Kind,
    /// The view box the text is fitted through.
    view_box: Option<[f32; 4]>,
    /// The revision.
    hash: u64,
}

/// Every drawing a window has produced, held between frames.
///
/// Held by the window rather than by the paint walk because the walk is a pure reader: it is handed
/// a source and does not own one. The interior mutability is what lets it stay a reader while this
/// still fills on demand.
#[derive(Debug, Default)]
pub struct VectorCache {
    /// The entries, by node.
    entries: RefCell<FxHashMap<NodeKey, Entry>>,
    /// Every source text an entry holds, by a hash of its kind and its bytes.
    ///
    /// Weak, so a text lives exactly as long as some node draws it. Equal text read for a thousand
    /// elements is one parse, and its shapes are one allocation every encoding cache recognises.
    parsed: RefCell<FxHashMap<u64, Weak<Parsed>>>,
    /// Each node's revision, until the node's text is replaced.
    revisions: RefCell<FxHashMap<NodeKey, Memo>>,
    /// How many drawings have been served from an entry that was already current.
    ///
    /// Monotonic and never reset: two readings subtracted answer "did anything draw from this
    /// between these two moments", which is the question a budget deciding whether the cache is
    /// cold asks. A drawing that had to be produced is not a hit — the cache did not save that
    /// frame anything.
    hits: Cell<u64>,
    /// How many revisions were hashed from the whole text.
    #[cfg(test)]
    hashes: Cell<u32>,
}

impl VectorCache {
    /// A cache holding nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many nodes have a drawing held for them.
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.entries.borrow().is_empty()
    }

    /// How many drawings have been served from an entry that was already current.
    pub fn hits(&self) -> u64 {
        self.hits.get()
    }

    /// Forgets every drawing, and reports how many that threw away.
    ///
    /// Nothing downstream is invalidated by this, which is what separates it from dropping a shaped
    /// paragraph: a drawing is fitted *from* the fragment it is drawn into, so the next frame that
    /// reaches one fits it again from the same box and gets the same picture. What it costs is a
    /// parse, and — because a rasteriser keys its encoding on the identity of the path allocation —
    /// a re-encode of every icon that comes back.
    pub fn clear(&mut self) -> usize {
        let held = self.entries.get_mut().len();
        self.entries.get_mut().clear();
        self.parsed.get_mut().clear();
        self.revisions.get_mut().clear();
        held
    }

    /// Forgets every node not in `live`, and every source text no node draws.
    ///
    /// Called when a frame ends. Without it a document that scrolled through a thousand icons keeps
    /// every one of them read for the life of the window.
    pub fn retain(&mut self, live: impl Fn(NodeKey) -> bool) {
        self.entries.get_mut().retain(|node, _| live(*node));
        self.revisions.get_mut().retain(|node, _| live(*node));
        self.parsed
            .get_mut()
            .retain(|_, parsed| parsed.strong_count() > 0);
    }

    /// The source one frame reads through, answered from `document`.
    pub fn frame<'a>(&'a self, document: &'a Document) -> Vectors<'a> {
        Vectors {
            cache: self,
            document,
        }
    }

    /// The source text `source` read as `kind`: the one held for an equal text, or a new read.
    ///
    /// `None` for a document that cannot be read.
    fn parsed(&self, kind: Kind, source: &SharedString) -> Option<Arc<Parsed>> {
        let key = zgui_scene::ContentHash::new()
            .u32(kind.tag())
            .bytes(source.as_bytes())
            .finish();
        if let Some(held) = self.parsed.borrow().get(&key).and_then(Weak::upgrade)
            && held.reads(kind, source)
        {
            return Some(held);
        }
        let (shapes, read) = match kind {
            Kind::Notation => (
                Arc::from(crate::content::vectors::outlines(
                    &parse(source),
                    Affine::IDENTITY,
                )),
                None,
            ),
            Kind::Document => {
                let read = Arc::new(zgui_svg::parse(source).ok()?);
                (read.shapes_shared(), Some(read))
            }
        };
        let parsed = Arc::new(Parsed {
            source: source.clone(),
            kind,
            shapes,
            read,
        });
        self.parsed
            .borrow_mut()
            .insert(key, Arc::downgrade(&parsed));
        Some(parsed)
    }

    /// The drawing held for `node`, when it was read from what `current` says and fitted by
    /// `placed`.
    fn current(
        &self,
        node: NodeKey,
        placed: [f64; 6],
        current: impl FnOnce(&Source) -> bool,
    ) -> Option<Drawing> {
        let entries = self.entries.borrow();
        let entry = entries.get(&node)?;
        (entry.placed == placed && current(&entry.source)).then(|| {
            self.hits.set(self.hits.get() + 1);
            entry.drawing.clone()
        })
    }

    /// Holds `drawing` for `node`, and hands it back.
    fn store(&self, node: NodeKey, source: Source, placed: [f64; 6], drawing: Drawing) -> Drawing {
        self.entries.borrow_mut().insert(
            node,
            Entry {
                source,
                placed,
                drawing: drawing.clone(),
            },
        );
        drawing
    }

    /// The drawing of a source text, fitted to `box_`.
    ///
    /// A text that cannot be read draws nothing rather than falling back to something else: the
    /// alternative is an element that silently draws a different picture from the one it was
    /// given, which is worse than an element that visibly draws none.
    fn read(
        &self,
        node: NodeKey,
        kind: Kind,
        source: &SharedString,
        view_box: Option<[f32; 4]>,
        box_: Placement,
    ) -> Option<Drawing> {
        // The held text first: a node whose property is the allocation it was read from needs
        // no hash of its text, and a node whose text changed reads its new one below.
        let held = match self.entries.borrow().get(&node).map(|entry| &entry.source) {
            Some(Source::Parsed(parsed)) if parsed.kind == kind && parsed.source.ptr_eq(source) => {
                Some(Arc::clone(parsed))
            }
            _ => None,
        };
        let parsed = match held {
            Some(parsed) => parsed,
            None => self.parsed(kind, source)?,
        };
        let view_box = match &parsed.read {
            Some(read) => Some(read.view_box()),
            None => view_box,
        };
        let placed = fit::onto(box_.content_box, view_box, box_.scale);
        let coefficients = placed.as_coeffs();
        if let Some(drawing) = self.current(node, coefficients, |held| match held {
            Source::Parsed(held) => Arc::ptr_eq(held, &parsed),
            Source::Canvas { .. } => false,
        }) {
            return Some(drawing);
        }
        let drawing = Drawing::fitted_shared(Arc::clone(&parsed.shapes), placed);
        Some(self.store(node, Source::Parsed(parsed), coefficients, drawing))
    }

    /// The shapes a retained canvas scene draws, with the fit that places them.
    ///
    /// The scene is resolved by token out of the paint-side registry. Its shapes are held as they
    /// are, and placed by the same fit a document's are only where a route asks for the placed
    /// path. The revision and the view ride in the source, so a mutated or panned scene misses
    /// the cache once and an untouched one hands back the same allocations for the encoding caches
    /// to recognise.
    ///
    /// A token whose scene has died draws nothing: the application dropped every handle while an
    /// element still named it, and inventing a picture for it would be worse than a blank.
    fn canvas(
        &self,
        node: NodeKey,
        token: u32,
        revision: u32,
        view: u64,
        view_box: Option<[f32; 4]>,
        box_: Placement,
    ) -> Option<Drawing> {
        let scene = zgui_canvas::resolve(zgui_canvas::CanvasToken(token))?;
        let placed = fit::onto(box_.content_box, view_box, box_.scale);
        let coefficients = placed.as_coeffs();
        let source = Source::Canvas {
            token,
            revision,
            view,
        };
        if let Some(drawing) = self.current(node, coefficients, |held| match held {
            Source::Canvas {
                token: held_token,
                revision: held_revision,
                view: held_view,
            } => (*held_token, *held_revision, *held_view) == (token, revision, view),
            Source::Parsed(_) => false,
        }) {
            return Some(drawing);
        }
        let drawing = {
            let scene = scene
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Drawing::canvas(&scene, placed)
        };
        Some(self.store(node, source, coefficients, drawing))
    }

    /// The revision of `node`'s text, hashed once for as long as the node holds that text.
    fn revision_of(
        &self,
        node: NodeKey,
        kind: Kind,
        source: &SharedString,
        view_box: Option<[f32; 4]>,
    ) -> u64 {
        if let Some(memo) = self.revisions.borrow().get(&node)
            && memo.kind == kind
            && memo.view_box == view_box
            && memo.source.ptr_eq(source)
        {
            return memo.hash;
        }
        #[cfg(test)]
        self.hashes.set(self.hashes.get() + 1);
        let mut hash = zgui_scene::ContentHash::new();
        if let Some(view_box) = view_box {
            hash = hash.f32s(&view_box);
        }
        let hash = hash.u32(kind.tag()).bytes(source.as_bytes()).finish();
        // The clone keeps the text's address unique while the memo stands, so a pointer-equal
        // text is this text.
        self.revisions.borrow_mut().insert(
            node,
            Memo {
                source: source.clone(),
                kind,
                view_box,
                hash,
            },
        );
        hash
    }

    /// How many source texts are held.
    #[cfg(test)]
    fn parsed_len(&self) -> usize {
        self.parsed.borrow().len()
    }
}

/// One frame's view of the cache, answered from the document the frame is painting.
#[derive(Clone, Copy)]
pub struct Vectors<'a> {
    /// Where drawings are kept.
    cache: &'a VectorCache,
    /// Where the sources are read from.
    document: &'a Document,
}

impl VectorSource for Vectors<'_> {
    /// A fingerprint of the drawing's source data: the canvas revision, the document text, or the
    /// path notation, together with the view box every one of them is fitted through.
    ///
    /// A text is hashed once, and its hash is held for as long as the node holds that very text:
    /// the revision is asked for every drawing fragment on every frame that reaches it.
    fn revision(&self, node: NodeKey) -> u64 {
        let store = self.document.store();
        let view_box = zgui_dom::side::drawing::view_box(store, node);
        if let Some((token, revision)) = zgui_dom::side::drawing::canvas(store, node) {
            let mut hash = zgui_scene::ContentHash::new();
            if let Some(view_box) = view_box {
                hash = hash.f32s(&view_box);
            }
            let view = zgui_dom::side::drawing::canvas_view(store, node).unwrap_or(0);
            return hash.u32(1).u32(token).u32(revision).u64(view).finish();
        }
        if let Some(source) = zgui_dom::side::drawing::document_text(store, node) {
            return self
                .cache
                .revision_of(node, Kind::Document, source, view_box);
        }
        if let Some(data) = zgui_dom::side::drawing::path_text(store, node) {
            return self.cache.revision_of(node, Kind::Notation, data, view_box);
        }
        0
    }

    /// A canvas wins over a document, and a document over path notation, when an element
    /// carries more than one.
    ///
    /// It has to be one of them and not several: each brings its own space with it, so drawing
    /// two would draw two pictures fitted by two different matrices into one box. In practice
    /// each source belongs to its own element name and the order is never exercised.
    fn drawing(&self, node: NodeKey, placement: Placement) -> Option<Drawing> {
        let store = self.document.store();
        let view_box = zgui_dom::side::drawing::view_box(store, node);
        if let Some((token, revision)) = zgui_dom::side::drawing::canvas(store, node) {
            let view = zgui_dom::side::drawing::canvas_view(store, node).unwrap_or(0);
            return self
                .cache
                .canvas(node, token, revision, view, view_box, placement);
        }
        if let Some(source) = zgui_dom::side::drawing::document_text(store, node) {
            return self
                .cache
                .read(node, Kind::Document, source, None, placement);
        }
        let data = zgui_dom::side::drawing::path_text(store, node)?;
        self.cache
            .read(node, Kind::Notation, data, view_box, placement)
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, NodeKind};
    use zgui_geom::{DevicePx, Point, Rect, Size};
    use zgui_interned::ElementName;
    use zgui_scene::kurbo::Shape;
    use zgui_vocab::{PropKey, PropValue, prop::drawing};

    use super::VectorCache;
    use crate::content::vectors::{Placement, VectorSource};

    /// A document with one `<vector>` carrying the given notation and view box.
    fn drawing_document(data: &str, view_box: Option<&str>) -> (Document, zgui_dom::NodeKey) {
        let mut document = Document::new();
        let index = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("vector"),
        );
        document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                edit.set_property(
                    index,
                    PropKey::new(drawing::PATHS),
                    Some(PropValue::from(data)),
                );
                if let Some(view_box) = view_box {
                    edit.set_property(
                        index,
                        PropKey::new(drawing::VIEW_BOX),
                        Some(PropValue::from(view_box)),
                    );
                }
            })
            .expect("not poisoned");
        let key = document.store().key_of(index);
        (document, key)
    }

    /// A placement over a box at the origin.
    fn placement(side: f32) -> Placement {
        Placement {
            content_box: Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(side), DevicePx(side)),
            ),
            scale: 1.0,
        }
    }

    #[test]
    fn a_drawing_is_read_off_the_element_and_fitted_to_its_box() {
        let (document, node) = drawing_document("M0 0 L24 0 L24 24 Z", Some("0 0 24 24"));
        let cache = VectorCache::new();
        let drawing = cache
            .frame(&document)
            .drawing(node, placement(48.0))
            .expect("the element draws");
        assert_eq!(drawing.shapes.len(), 1);
        assert_eq!(drawing.fit, zgui_scene::kurbo::Affine::scale(2.0));
        assert_eq!(
            drawing.placed(0).path.bounding_box().width(),
            48.0,
            "a twenty-four unit outline in a forty-eight pixel box is drawn at twice the size"
        );
    }

    /// The whole reason the cache exists: a rasteriser recognises geometry by the identity of the
    /// allocation, so a second frame that produced a new one would re-encode every icon on screen.
    #[test]
    fn the_same_drawing_in_the_same_box_hands_back_the_same_allocation() {
        let (document, node) = drawing_document("M0 0 L24 0 L24 24 Z", Some("0 0 24 24"));
        let cache = VectorCache::new();
        let first = cache
            .frame(&document)
            .drawing(node, placement(48.0))
            .unwrap();
        let second = cache
            .frame(&document)
            .drawing(node, placement(48.0))
            .unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &first.shapes[0].path,
            &second.shapes[0].path
        ));
    }

    #[test]
    fn a_drawing_placed_into_a_different_box_is_placed_again() {
        let (document, node) = drawing_document("M0 0 L24 0 L24 24 Z", Some("0 0 24 24"));
        let cache = VectorCache::new();
        let small = cache
            .frame(&document)
            .drawing(node, placement(16.0))
            .unwrap();
        let large = cache
            .frame(&document)
            .drawing(node, placement(48.0))
            .unwrap();
        assert_ne!(small.fit, large.fit);
        assert!(
            std::sync::Arc::ptr_eq(&small.shapes[0].path, &large.shapes[0].path),
            "both boxes draw the one source path"
        );
        assert_eq!(small.placed(0).path.bounding_box().width(), 16.0);
        assert_eq!(large.placed(0).path.bounding_box().width(), 48.0);
    }

    /// An icon swapped for another of the same size occupies exactly the same box, so nothing but
    /// the notation itself can tell the cache the held curves are stale.
    #[test]
    fn changing_the_outlines_places_them_again() {
        let (document, node) = drawing_document("M0 0 L24 0 L24 24 Z", Some("0 0 24 24"));
        let cache = VectorCache::new();
        let before = cache
            .frame(&document)
            .drawing(node, placement(24.0))
            .unwrap();

        let index = document.store().index_of(node).expect("a live node");
        document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                edit.set_property(
                    index,
                    PropKey::new(drawing::PATHS),
                    Some(PropValue::from("M24 0 L24 24 L0 24 Z")),
                );
            })
            .expect("not poisoned");

        let after = cache
            .frame(&document)
            .drawing(node, placement(24.0))
            .unwrap();
        assert!(!std::sync::Arc::ptr_eq(
            &before.shapes[0].path,
            &after.shapes[0].path
        ));
    }

    /// A document with two elements of `name` carrying `value` in `property`, each from its own
    /// allocation.
    fn two_elements(name: &str, property: &str, value: &str) -> (Document, [zgui_dom::NodeKey; 2]) {
        let mut document = Document::new();
        let mut keys = Vec::new();
        for _ in 0..2 {
            let index = document.append(
                document.document_index(),
                NodeKind::Element,
                ElementName::new(name),
            );
            document
                .edit(&zgui_dom::EverythingMatters, |edit| {
                    edit.set_property(
                        index,
                        PropKey::new(property),
                        Some(PropValue::from(value.to_owned())),
                    );
                })
                .expect("not poisoned");
            keys.push(document.store().key_of(index));
        }
        (document, [keys[0], keys[1]])
    }

    #[test]
    fn one_notation_is_parsed_once_for_every_element() {
        let (document, [one, two]) = two_elements("vector", drawing::PATHS, "M0 0 L24 0 L24 24 Z");
        let cache = VectorCache::new();
        let first = cache
            .frame(&document)
            .drawing(one, placement(24.0))
            .unwrap();
        let second = cache
            .frame(&document)
            .drawing(two, placement(48.0))
            .unwrap();
        assert!(
            std::sync::Arc::ptr_eq(&first.shapes, &second.shapes),
            "equal notation in two elements is one read and one allocation"
        );
        assert_eq!(cache.parsed_len(), 1);
    }

    #[test]
    fn one_document_is_read_once_for_every_element() {
        let source = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M0 0 L24 0 L24 24 Z" fill="red"/></svg>"#;
        let (document, [one, two]) = two_elements("vector", drawing::DOCUMENT, source);
        let cache = VectorCache::new();
        let first = cache
            .frame(&document)
            .drawing(one, placement(24.0))
            .unwrap();
        let second = cache
            .frame(&document)
            .drawing(two, placement(24.0))
            .unwrap();
        assert!(std::sync::Arc::ptr_eq(&first.shapes, &second.shapes));
        assert_eq!(cache.parsed_len(), 1);
    }

    #[test]
    fn a_dead_source_leaves_the_parsed_map() {
        let (document, [one, two]) = two_elements("vector", drawing::PATHS, "M0 0 L24 0 L24 24 Z");
        let mut cache = VectorCache::new();
        let drawn = |cache: &VectorCache, node| {
            cache.frame(&document).drawing(node, placement(24.0));
        };
        drawn(&cache, one);
        drawn(&cache, two);
        cache.retain(|node| node == two);
        assert_eq!(cache.parsed_len(), 1, "a node still draws the text");
        cache.retain(|_| false);
        assert_eq!(cache.parsed_len(), 0, "no node draws it any more");
    }

    #[test]
    fn a_revision_is_hashed_once_per_source_text() {
        let (document, node) = drawing_document("M0 0 L24 0 L24 24 Z", Some("0 0 24 24"));
        let cache = VectorCache::new();
        let first = cache.frame(&document).revision(node);
        let second = cache.frame(&document).revision(node);
        assert_eq!(first, second);
        assert_eq!(cache.hashes.get(), 1, "the held text is hashed once");

        let index = document.store().index_of(node).expect("a live node");
        document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                edit.set_property(
                    index,
                    PropKey::new(drawing::PATHS),
                    Some(PropValue::from("M24 0 L24 24 L0 24 Z")),
                );
            })
            .expect("not poisoned");
        let third = cache.frame(&document).revision(node);
        assert_ne!(third, first, "new text is a new revision");
        assert_eq!(cache.hashes.get(), 2);
    }

    /// A document with one `<canvas>` naming the given scene.
    fn canvas_document(handle: &zgui_canvas::SceneHandle) -> (Document, zgui_dom::NodeKey) {
        let mut document = Document::new();
        let index = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("canvas"),
        );
        let key = document.store().key_of(index);
        write_reference(&document, index, handle);
        (document, key)
    }

    /// Writes the scene's current token-and-revision and its view onto the element, as the
    /// binding would.
    fn write_reference(
        document: &Document,
        index: zgui_dom::NodeIndex,
        handle: &zgui_canvas::SceneHandle,
    ) {
        document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                edit.set_property(
                    index,
                    PropKey::new(drawing::CANVAS),
                    Some(PropValue::Integer(drawing::canvas_value(
                        handle.token().0,
                        handle.revision(),
                    ))),
                );
                edit.set_property(
                    index,
                    PropKey::new(drawing::CANVAS_VIEW),
                    Some(PropValue::Integer(handle.view() as i64)),
                );
            })
            .expect("not poisoned");
    }

    /// An eight-unit triangle with its own solid fill.
    fn triangle() -> zgui_canvas::Shape {
        let mut path = zgui_scene::kurbo::BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((8.0, 0.0));
        path.line_to((4.0, 8.0));
        path.close_path();
        zgui_canvas::ShapeBuilder::new(path)
            .fill(zgui_canvas::Brush::Solid(zgui_color::Color::srgb(
                1.0, 0.0, 0.0, 1.0,
            )))
            .build()
    }

    #[test]
    fn a_canvas_scene_is_resolved_placed_and_held_by_its_revision() {
        let handle = zgui_canvas::SceneHandle::new();
        handle.edit(|scene| scene.push(triangle()));
        let (document, node) = canvas_document(&handle);
        let cache = VectorCache::new();

        let first = cache
            .frame(&document)
            .drawing(node, placement(24.0))
            .expect("the canvas draws");
        assert_eq!(first.shapes.len(), 1);
        assert!(
            first.shapes[0].fill.is_some(),
            "the shape's own paint survives the crossing"
        );
        let second = cache
            .frame(&document)
            .drawing(node, placement(24.0))
            .expect("still draws");
        assert!(
            std::sync::Arc::ptr_eq(&first.shapes[0].path, &second.shapes[0].path),
            "an untouched canvas in an untouched box is the same allocation, which is what the \
             rasteriser's encoding cache keys on"
        );

        // A mutation moves the revision; once the binding rewrites the property, the cache misses
        // exactly once and the new shape arrives.
        handle.edit(|scene| {
            scene.clear();
            scene.push(triangle());
        });
        let index = document.store().index_of(node).expect("live");
        write_reference(&document, index, &handle);
        let third = cache
            .frame(&document)
            .drawing(node, placement(24.0))
            .expect("still draws");
        assert!(!std::sync::Arc::ptr_eq(
            &first.shapes[0].path,
            &third.shapes[0].path
        ));
    }

    #[test]
    fn a_dead_scene_draws_nothing_rather_than_something_else() {
        let handle = zgui_canvas::SceneHandle::new();
        handle.edit(|scene| scene.push(triangle()));
        let (document, node) = canvas_document(&handle);
        let cache = VectorCache::new();
        assert!(
            cache
                .frame(&document)
                .drawing(node, placement(24.0))
                .is_some()
        );
        drop(handle);
        // The held entry is not consulted for a token that no longer resolves: the application
        // dropped the scene, and a picture served from a cache it can no longer reach is a
        // picture it can never change again.
        assert!(
            cache
                .frame(&document)
                .drawing(node, placement(24.0))
                .is_none()
        );
    }

    #[test]
    fn an_element_that_draws_nothing_has_no_drawing() {
        let mut document = Document::new();
        let index = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("box"),
        );
        let node = document.store().key_of(index);
        let cache = VectorCache::new();
        assert!(
            cache
                .frame(&document)
                .drawing(node, placement(24.0))
                .is_none()
        );
    }

    #[test]
    fn forgetting_drops_the_nodes_a_frame_did_not_draw() {
        let (document, node) = drawing_document("M0 0 L24 0", None);
        let mut cache = VectorCache::new();
        cache.frame(&document).drawing(node, placement(24.0));
        assert_eq!(cache.len(), 1);
        cache.retain(|_| false);
        assert!(cache.is_empty());
    }

    #[test]
    fn a_view_change_composes_into_the_fit() {
        let handle = zgui_canvas::SceneHandle::new();
        handle.edit(|scene| scene.push(triangle()));
        let source = handle.edit(|scene| std::sync::Arc::clone(&scene.shapes()[0].path));
        let (document, node) = canvas_document(&handle);
        let cache = VectorCache::new();
        let mut box_ = placement(24.0);
        box_.content_box.origin.x = DevicePx(10.0);
        let before = cache.frame(&document).drawing(node, box_).expect("draws");
        assert_eq!(
            before.fit,
            zgui_scene::kurbo::Affine::translate((10.0, 0.0))
        );

        let view = zgui_scene::kurbo::Affine::translate((3.0, 4.0))
            * zgui_scene::kurbo::Affine::scale(2.0);
        let revision = handle.revision();
        assert!(handle.set_transform(view));
        assert_eq!(handle.revision(), revision, "a view moves no revision");
        let index = document.store().index_of(node).expect("live");
        let signature = cache.frame(&document).revision(node);
        write_reference(&document, index, &handle);
        assert_ne!(
            cache.frame(&document).revision(node),
            signature,
            "a pan is a new record signature"
        );
        let after = cache.frame(&document).drawing(node, box_).expect("draws");
        assert_eq!(
            after.fit,
            zgui_scene::kurbo::Affine::translate((10.0, 0.0)) * view,
            "the fit is the box fit times the view"
        );
        assert!(
            std::sync::Arc::ptr_eq(&after.shapes[0].path, &source),
            "the view keeps the source path"
        );
    }

    #[test]
    fn a_canvas_drawing_carries_its_series() {
        let handle = zgui_canvas::SceneHandle::new();
        let data: std::sync::Arc<[[f32; 2]]> = std::sync::Arc::from([[0.0, 0.0], [1.0, 2.0]]);
        handle.edit(|scene| {
            scene.push(triangle());
            scene.push_series(zgui_canvas::Series::Points {
                data: std::sync::Arc::clone(&data),
                to_canvas: zgui_scene::kurbo::Affine::IDENTITY,
                marker: zgui_canvas::Marker::Circle { radius: 2.0 },
                fill: Some(zgui_canvas::Brush::Inherited { alpha: 1.0 }),
                stroke: None,
            });
        });
        let (document, node) = canvas_document(&handle);
        let cache = VectorCache::new();
        let drawing = cache
            .frame(&document)
            .drawing(node, placement(24.0))
            .expect("draws");
        assert_eq!(drawing.series.len(), 1);
        assert_eq!(drawing.series[0].before, 1);
        let zgui_canvas::Series::Points { data: held, .. } = &drawing.series[0].series else {
            panic!("a points series");
        };
        assert!(std::sync::Arc::ptr_eq(held, &data), "the data is shared");
    }

    #[test]
    fn a_canvas_drawing_keeps_its_source_and_places_on_demand() {
        let handle = zgui_canvas::SceneHandle::new();
        handle.edit(|scene| scene.push(triangle()));
        let source = handle.edit(|scene| std::sync::Arc::clone(&scene.shapes()[0].path));
        let (document, node) = canvas_document(&handle);
        let cache = VectorCache::new();
        let mut box_ = placement(24.0);
        box_.content_box.origin.x = DevicePx(10.0);
        let drawing = cache
            .frame(&document)
            .drawing(node, box_)
            .expect("the canvas draws");
        assert!(
            std::sync::Arc::ptr_eq(&drawing.shapes[0].path, &source),
            "the drawing holds the scene's own path"
        );
        assert!(
            drawing.cell(0).and_then(|cell| cell.get()).is_none(),
            "nothing is placed before a route asks"
        );
        let placed = std::sync::Arc::clone(&drawing.placed(0).path);
        assert_eq!(placed.bounding_box().x0, 10.0, "placed by the fit");
        assert!(std::sync::Arc::ptr_eq(&placed, &drawing.placed(0).path));
        let next = cache
            .frame(&document)
            .drawing(node, box_)
            .expect("still draws");
        assert!(
            std::sync::Arc::ptr_eq(&placed, &next.placed(0).path),
            "the next frame shares the placed path"
        );
    }
}
