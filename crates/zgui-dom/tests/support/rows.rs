//! A list of rows, and the from-scratch oracle every structural case is judged against.
//!
//! The oracle is a second document built to the *final* shape directly and styled once. It is the
//! only instrument that can see a missed sibling invalidation, because both halves of an
//! incremental comparison share the same incremental path: if the change failed to invalidate a
//! sibling, the sibling keeps a stale computed value, and only a document that never had the stale
//! value in the first place disagrees with it.

use zgui_dom::{Document, EverythingMatters, NodeIndex, NodeKind};
use zgui_interned::{ClassName, ElementName};

use crate::support::engine::Engine;
use crate::support::read::radius;

/// A container with a row of element children.
pub(crate) struct Rows {
    /// The document.
    pub(crate) document: Document,
    /// The container the rows are children of.
    pub(crate) container: NodeIndex,
    /// The rows, in the order they were built.
    pub(crate) rows: Vec<NodeIndex>,
}

impl Rows {
    /// A container holding `count` rows.
    pub(crate) fn new(count: usize) -> Self {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let container = document.append(root, NodeKind::Element, ElementName::new("ul"));
        document.set_classes(container, &[ClassName::new("list")]);
        let rows = (0..count)
            .map(|_| {
                let row = document.append(container, NodeKind::Element, ElementName::new("li"));
                document.set_classes(row, &[ClassName::new("row")]);
                row
            })
            .collect();
        Self {
            document,
            container,
            rows,
        }
    }

    /// Styles the document once with `sheet` and hands back the engine that did it.
    ///
    /// The first pass is what makes the container carry the selector flags a structural change
    /// reads: the engine records them while it matches, and a document that has never been matched
    /// has none.
    pub(crate) fn styled(&mut self, sheet: &str) -> Engine {
        let mut engine = Engine::new(&self.document);
        engine.add_author_sheet(sheet);
        engine.restyle(&mut self.document, None);
        crate::support::edit::retire(&mut self.document);
        engine
    }

    /// Creates a detached row carrying the `row` class.
    pub(crate) fn new_row(&self) -> NodeIndex {
        self.document
            .edit(&EverythingMatters, |edit| {
                let row = edit.create_element(ElementName::new("li"));
                edit.set_classes(row, &[ClassName::new("row")]);
                row
            })
            .expect("the document is not poisoned")
    }

    /// The computed corner radius of every element child of the container, in document order.
    ///
    /// A reset property, deliberately: an inherited one cannot tell an element a rule matched from
    /// an element that merely inherited from one.
    pub(crate) fn radii(&self) -> Vec<f32> {
        let mut out = Vec::new();
        let mut current = self
            .document
            .store()
            .core(self.container)
            .first_element_child();
        while let Some(index) = current {
            out.push(radius(&self.document, index));
            current = self.document.store().core(index).next_element();
        }
        out
    }
}

/// The radii a container of `count` rows has when it is built to that shape and styled once.
pub(crate) fn oracle(count: usize, sheet: &str) -> Vec<f32> {
    let mut fresh = Rows::new(count);
    fresh.styled(sheet);
    fresh.radii()
}
