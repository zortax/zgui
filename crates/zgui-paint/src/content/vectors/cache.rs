//! The placed outlines one window has already produced, kept between frames.

use core::cell::{Cell, RefCell};
use std::sync::Arc;

use rustc_hash::FxHashMap;
use zgui_dom::{Document, NodeKey};
use zgui_scene::kurbo::Affine;

use crate::content::vectors::{Drawing, Placement, VectorSource, parse};
use crate::emit::vector::fit;

/// What was produced for one node, and what it was produced from.
#[derive(Clone, Debug)]
struct Entry {
    /// The notation or the document source the outlines were read from.
    data: String,
    /// The read document, kept so that re-fitting into a different box re-places the outlines
    /// rather than re-reading the source.
    ///
    /// This is what makes a document cost one read however many sizes it is drawn at, and it is
    /// why the read is colour-independent: nothing about the element's colour is in this key, so a
    /// hover that re-colours an icon re-places nothing and re-reads nothing.
    read: Option<Arc<zgui_svg::Document>>,
    /// The matrix they were placed with, as its six coefficients — compared rather than the box and
    /// the view box separately, because two different boxes that fit to the same matrix produce the
    /// same curves and re-placing them would throw away an encoding for nothing.
    placed: [f64; 6],
    /// The result.
    drawing: Drawing,
}

/// Every drawing a window has placed, held between frames.
///
/// Held by the window rather than by the paint walk because the walk is a pure reader: it is handed
/// a source and does not own one. The interior mutability is what lets it stay a reader while this
/// still fills on demand.
#[derive(Debug, Default)]
pub struct VectorCache {
    /// The entries, by node.
    entries: RefCell<FxHashMap<NodeKey, Entry>>,
    /// How many drawings have been served from an entry that was already current.
    ///
    /// Monotonic and never reset: two readings subtracted answer "did anything draw from this
    /// between these two moments", which is the question a budget deciding whether the cache is
    /// cold asks. A placement that had to be produced is not a hit — the cache did not save that
    /// frame anything.
    hits: Cell<u64>,
}

impl VectorCache {
    /// A cache holding nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many nodes have a placed drawing held for them.
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

    /// Forgets every placed drawing, and reports how many that threw away.
    ///
    /// Nothing downstream is invalidated by this, which is what separates it from dropping a shaped
    /// paragraph: a drawing is placed *from* the fragment it is drawn into, so the next frame that
    /// reaches one places it again from the same box and gets the same curves. What it costs is a
    /// parse and a fit, and — because a rasteriser keys its encoding on the identity of the path
    /// allocation — a re-encode of every icon that comes back.
    pub fn clear(&mut self) -> usize {
        let held = self.entries.get_mut().len();
        self.entries.get_mut().clear();
        held
    }

    /// Forgets every node not in `live`.
    ///
    /// Called when a frame ends. Without it a document that scrolled through a thousand icons keeps
    /// every one of them placed for the life of the window.
    pub fn retain(&mut self, live: impl Fn(NodeKey) -> bool) {
        self.entries.get_mut().retain(|node, _| live(*node));
    }

    /// The source one frame reads through, answered from `document`.
    pub fn frame<'a>(&'a self, document: &'a Document) -> Vectors<'a> {
        Vectors {
            cache: self,
            document,
        }
    }

    /// The document held for `node`, if it was read from this very source.
    fn held(&self, node: NodeKey, data: &str) -> Option<Arc<zgui_svg::Document>> {
        let entries = self.entries.borrow();
        let entry = entries.get(&node)?;
        (entry.data == data).then(|| entry.read.clone())?
    }

    /// The placed shapes for `data`, produced only if what is held is stale.
    ///
    /// `place` is called only when there is nothing current to hand back, which is what makes a
    /// drawing on the screen for a thousand frames cost one placement.
    fn store(
        &self,
        node: NodeKey,
        data: &str,
        placed: Affine,
        read: Option<Arc<zgui_svg::Document>>,
        place: impl FnOnce() -> Vec<zgui_svg::Shape>,
    ) -> Drawing {
        let coefficients = placed.as_coeffs();
        {
            let entries = self.entries.borrow();
            if let Some(entry) = entries.get(&node)
                && entry.data == data
                && entry.placed == coefficients
            {
                self.hits.set(self.hits.get() + 1);
                return entry.drawing.clone();
            }
        }
        let drawing = Drawing { shapes: place() };
        self.entries.borrow_mut().insert(
            node,
            Entry {
                data: data.to_owned(),
                read,
                placed: coefficients,
                drawing: drawing.clone(),
            },
        );
        drawing
    }

    /// The placed outlines a list of path notation draws.
    fn notated(&self, node: NodeKey, data: &str, placed: Affine) -> Drawing {
        self.store(node, data, placed, None, || {
            crate::content::vectors::outlines(&parse(data), placed)
        })
    }

    /// The placed outlines a vector document draws.
    ///
    /// A document that cannot be read draws nothing rather than falling back to something else:
    /// the alternative is an element that silently draws a different picture from the one it was
    /// given, which is worse than an element that visibly draws none.
    fn documented(&self, node: NodeKey, source: &str, box_: Placement) -> Option<Drawing> {
        let read = match self.held(node, source) {
            Some(held) => held,
            None => Arc::new(zgui_svg::parse(source).ok()?),
        };
        let placed = fit::onto(box_.content_box, Some(read.view_box()), box_.scale);
        let shapes = read.clone();
        Some(self.store(node, source, placed, Some(read), move || {
            shapes.placed(placed)
        }))
    }
}

/// One frame's view of the cache, answered from the document the frame is painting.
#[derive(Clone, Copy)]
pub struct Vectors<'a> {
    /// Where placed outlines are kept.
    cache: &'a VectorCache,
    /// Where the notation is read from.
    document: &'a Document,
}

impl VectorSource for Vectors<'_> {
    /// A document wins over path notation when an element carries both.
    ///
    /// It has to be one of them and not both: a document brings its own space with it, so drawing
    /// the notation as well would draw two pictures fitted by two different matrices into one box.
    fn drawing(&self, node: NodeKey, placement: Placement) -> Option<Drawing> {
        let store = self.document.store();
        if let Some(source) = zgui_dom::side::drawing::document(store, node) {
            return self.cache.documented(node, source, placement);
        }
        let data = zgui_dom::side::drawing::path_data(store, node)?;
        let view_box = zgui_dom::side::drawing::view_box(store, node);
        let placed = fit::onto(placement.content_box, view_box, placement.scale);
        Some(self.cache.notated(node, data, placed))
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
        assert_eq!(
            drawing.shapes[0].path.bounding_box().width(),
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
        assert!(!std::sync::Arc::ptr_eq(
            &small.shapes[0].path,
            &large.shapes[0].path
        ));
        assert_eq!(small.shapes[0].path.bounding_box().width(), 16.0);
        assert_eq!(large.shapes[0].path.bounding_box().width(), 48.0);
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
}
