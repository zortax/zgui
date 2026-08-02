//! The node-tree seam, over a real document.

mod attrs;
mod build;
mod nodes;
mod query;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_dom::{Document, EverythingMatters, NodeIndex, StyleFilter};
use zgui_view::{DocumentId, NodeId};

use crate::handlers::{Handler, Handlers};
use crate::id;
use crate::observations::Observations;

pub use crate::dom::build::Roots;

/// A [`Dom`](zgui_view::Dom) over a real document.
///
/// Every change it makes goes through the document's own batch API, which is what keeps the style
/// engine's invalidation protocol intact: a change records what the element looked like before,
/// tells the ancestors there is work below them, and asks for the frame that will show it. There
/// is no path through this crate that writes a node any other way.
///
/// # How the document is shared
///
/// A view calls into this through a shared reference, from anywhere, including from inside a
/// listener that is itself running inside a change. So the document is behind a shared cell that
/// every one of these methods borrows *immutably* — nested immutable borrows are ordinary, and a
/// change joins whatever batch is already open rather than starting a second one.
///
/// The frame's own stages are the exception, and they are the reason the cell is there: a restyle
/// and the end-of-frame recycle both need the document exclusively, and both run at a point where
/// no view is building and no listener is dispatching. [`DocumentDom::document`] is the shared
/// borrow; whoever drives frames holds the cell and takes the exclusive one itself.
///
/// ```
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use zgui_dom::Document;
/// use zgui_interned::ElementName;
/// use zgui_view::Dom;
/// use zgui_view_dom::DocumentDom;
///
/// let document = Rc::new(RefCell::new(Document::new()));
/// let dom = DocumentDom::new(Rc::clone(&document));
///
/// let row = dom.create_element(ElementName::new("row"));
/// let text = dom.create_text("hello");
/// dom.insert(row, text, None);
/// dom.insert(dom.root(row), row, None);
///
/// assert_eq!(dom.parent(text), Some(row));
/// assert_eq!(document.borrow().store().core(dom.index_of(row)).child_count(), 1);
/// ```
pub struct DocumentDom {
    /// The document being driven.
    document: Rc<RefCell<Document>>,
    /// Which document this is, as the view layer numbers them.
    id: DocumentId,
    /// Which changes can affect a computed style, answered by the installed rule set.
    filter: RefCell<Rc<dyn StyleFilter>>,
    /// The window's root element and the four overlay layers under its overlay root.
    roots: Roots,
    /// The handlers behind the registrations the document holds.
    handlers: Rc<RefCell<Handlers>>,
    /// Who is watching which of a node's measurements.
    observations: Rc<RefCell<Observations>>,
}

impl DocumentDom {
    /// Builds a backend over `document`, creating the tree shape every window has.
    ///
    /// That shape is a root element and, under it, an overlay root holding one node per overlay
    /// layer in ascending order. They are created here rather than on demand because the framework's
    /// own style sheet is written against them: the layer nodes are what carry the ordering between
    /// a menu, a dialog and a toast, and a layer created when its first portal appears would order
    /// them by whichever opened first.
    pub fn new(document: Rc<RefCell<Document>>) -> Self {
        let (id, roots) = {
            let borrowed = document.borrow();
            let id = DocumentId::new(borrowed.store().document().get())
                .expect("a document identifier fits in both numberings");
            let roots = Roots::create(&borrowed);
            (id, roots)
        };
        Self {
            document,
            id,
            filter: RefCell::new(Rc::new(EverythingMatters)),
            roots,
            handlers: Rc::new(RefCell::new(Handlers::new())),
            observations: Rc::new(RefCell::new(Observations::new())),
        }
    }

    /// Which document this drives, as the view layer numbers documents.
    pub fn document_id(&self) -> DocumentId {
        self.id
    }

    /// The document, borrowed for as long as the returned guard lives.
    pub fn document(&self) -> std::cell::Ref<'_, Document> {
        self.document.borrow()
    }

    /// Installs the answer to "can a change of this kind affect any computed style".
    ///
    /// Until a rule set says otherwise the answer is always yes, which is correct and slow: every
    /// class toggle and every state bit enters the style engine. Installing the rule set's own
    /// filter is what turns the changes no selector mentions into changes that cost nothing.
    pub fn set_style_filter(&self, filter: Rc<dyn StyleFilter>) {
        *self.filter.borrow_mut() = filter;
    }

    /// The handler `id` names, for whoever is dispatching an event to it.
    pub fn handler(&self, id: zgui_dom::side::listeners::ListenerId) -> Option<Handler> {
        self.handlers.borrow().get(id)
    }

    /// How many handlers this backend is holding.
    pub fn handler_count(&self) -> usize {
        self.handlers.borrow().len()
    }

    /// How many measurements are being watched, across every node.
    pub fn observation_count(&self) -> usize {
        self.observations.borrow().len()
    }

    /// Every node something is watching, each named once.
    pub fn observed_nodes(&self) -> Vec<NodeId> {
        self.observations.borrow().nodes()
    }

    /// Hands a settled measurement to everything watching it.
    ///
    /// Called once per changed measurement after layout has settled and before anything is
    /// painted, so a view that repositions itself from what it observes is painted in its final
    /// place in the same frame.
    pub fn deliver(&self, node: NodeId, value: zgui_view::ObservedValue) {
        // Not borrowed across the call: a sink writes a signal, and a signal write can reach code
        // that registers an observation of its own.
        let observations = self.observations.borrow();
        observations.deliver(node, value);
    }

    /// The document's own name for `node`.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not a live node of this document.
    pub fn index_of(&self, node: NodeId) -> NodeIndex {
        debug_assert!(
            node.belongs_to(self.id),
            "a handle minted by another window was applied to this one"
        );
        id::resolve(&self.document.borrow(), node)
    }

    /// The same, for a caller that has to cope with a node that has already gone.
    ///
    /// Most of what a view does to an element is meaningless if the element is not there, and
    /// [`DocumentDom::index_of`]'s panic says so. Releasing something the element *held* is the
    /// exception: a guard is dropped when the scope that installed it is disposed of, and a
    /// subtree's nodes are removed before its scopes are, so by then the node is expected to be
    /// gone. Answering `None` is what lets a release path skip the document and still run the half
    /// of its work that does not live there.
    pub fn live_index_of(&self, node: NodeId) -> Option<NodeIndex> {
        debug_assert!(
            node.belongs_to(self.id),
            "a handle minted by another window was applied to this one"
        );
        let document = self.document.borrow();
        id::to_document(node).and_then(|key| document.store().index_of(key))
    }

    /// Runs `body` as a batch of changes on the document.
    ///
    /// # Panics
    ///
    /// Panics if the document has been poisoned by a change that unwound. There is no recovery
    /// from that and no useful thing to do with a document that describes neither its old state
    /// nor its new one, so it is reported where it happened rather than carried further.
    pub(crate) fn edit<R>(&self, body: impl FnOnce(&mut zgui_dom::Edit<'_>) -> R) -> R {
        let filter = Rc::clone(&self.filter.borrow());
        self.document
            .borrow()
            .edit(filter.as_ref(), body)
            .expect("the document has not been poisoned")
    }

    /// The handlers, for the methods that add and remove them.
    pub(crate) fn handlers(&self) -> &RefCell<Handlers> {
        &self.handlers
    }

    /// The observers, shared so a deregistration can reach them without keeping the whole backend
    /// alive.
    pub(crate) fn observations_shared(&self) -> Rc<RefCell<Observations>> {
        Rc::clone(&self.observations)
    }

    /// The document, weakly, for the same reason.
    pub(crate) fn document_weak(&self) -> std::rc::Weak<RefCell<Document>> {
        Rc::downgrade(&self.document)
    }

    /// The observers, for the methods that add and remove them.
    pub(crate) fn observations(&self) -> &RefCell<Observations> {
        &self.observations
    }

    /// Ends the frame: drops what was taken out of the document during it, and forgets everything
    /// this backend was holding on its behalf.
    ///
    /// The two halves are one call because they are one obligation. A node taken out of the
    /// document keeps its record for the rest of the frame, so its listeners' handlers and its
    /// observers' channels are still reachable and still correct; the moment the record goes, both
    /// become entries that name nothing. Ending the frame without this leaks one handler per
    /// listener and one channel per observer of every view that has ever unmounted.
    ///
    /// # Panics
    ///
    /// Panics if a view is mid-build or a listener is mid-dispatch, which is what taking the
    /// document exclusively while either is in flight would mean. Call it where the frame ends.
    pub fn end_frame(&self) {
        let mut document = self.document.borrow_mut();
        zgui_dom::arena::end_frame(&mut document);
        self.handlers.borrow_mut().retain_live(&document);
        self.observations.borrow_mut().retain_live(&document);
    }

    /// The window's root and its overlay layers.
    pub(crate) fn roots(&self) -> &Roots {
        &self.roots
    }

    /// The window's root element.
    ///
    /// Everything a view builds goes under this. It exists before any view does, because a view
    /// can only ever attach a listener to a node it created and outside-press dismissal needs one
    /// on a node it did not.
    pub fn root_node(&self) -> NodeId {
        self.roots.root()
    }

    /// The node portalled content on `layer` goes into.
    pub fn overlay_layer(&self, layer: zgui_view::OverlayLayer) -> NodeId {
        self.roots.layer(layer)
    }
}
