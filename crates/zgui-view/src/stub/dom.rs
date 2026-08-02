//! An in-memory node tree.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use zgui_interned::{AttrName, ClassName, CustomPropertyName, ElementName};
use zgui_vocab::{EventKind, ListenerOptions, PropKey, PropValue, Semantics, UiState};

use crate::dom::{
    Dom, ListenerId, ObservationHandle, ObservationSink, Observed, ObservedValue, OverlayLayer,
};
use crate::event::EventCx;
use crate::id::{DocumentId, NodeId};
use crate::stub::node::{StubKind, StubListener, StubNode};

/// Everything the tree keeps, behind one borrow.
#[derive(Default)]
struct Tree {
    /// The nodes, by handle.
    nodes: BTreeMap<NodeId, StubNode>,
    /// The next backend number to mint.
    next: u64,
    /// The next listener number to mint.
    next_listener: u64,
    /// The window's root element, created on first use.
    root: Option<NodeId>,
    /// The overlay root of each band, created on first use.
    overlays: BTreeMap<OverlayLayer, NodeId>,
    /// The live observations, by the number that deregisters them.
    observations: BTreeMap<u64, (NodeId, Observed, ObservationSink)>,
    /// The next observation number to mint.
    next_observation: u64,
}

/// A [`Dom`] over an in-memory tree.
///
/// It exists so that this crate's examples and tests have a backend to build against, and so that
/// anyone writing a new backend has a small, complete one to read. It keeps a tree and answers
/// questions about it; it records no transcript and asserts nothing.
///
/// ```
/// use zgui_interned::{ClassName, ElementName};
/// use zgui_view::stub::StubDom;
/// use zgui_view::{DocumentId, Dom};
///
/// let dom = StubDom::new(DocumentId::FIRST);
/// let row = dom.create_element(ElementName::new("row"));
/// let first = dom.create_text("a");
/// let second = dom.create_text("b");
///
/// dom.insert(row, second, None);
/// dom.insert(row, first, Some(second));
/// assert_eq!(dom.children(row), vec![first, second]);
///
/// dom.toggle_class(row, ClassName::new("busy"), true);
/// assert_eq!(dom.classes(row), vec![ClassName::new("busy")]);
/// ```
pub struct StubDom {
    /// Which document this tree is.
    document: DocumentId,
    /// The tree itself, behind a reference count so that an observation's deregistration can
    /// hold it weakly and do nothing at all once the tree is gone.
    tree: Rc<RefCell<Tree>>,
}

impl StubDom {
    /// An empty tree belonging to `document`.
    pub fn new(document: DocumentId) -> Self {
        Self {
            document,
            tree: Rc::new(RefCell::new(Tree {
                next: 1,
                next_listener: 1,
                next_observation: 1,
                ..Tree::default()
            })),
        }
    }

    /// Which document this tree is.
    pub fn document(&self) -> DocumentId {
        self.document
    }

    /// Declares which node is this window's root, which is what [`Dom::root`] then answers.
    ///
    /// Without it the first caller of [`Dom::root`] mints one, which is fine for a tree nobody
    /// built a view into and wrong for a harness that already made a root and mounted everything
    /// under it: the view's own root and the one a dismissable overlay listens on would be two
    /// different elements, and the press would never be heard.
    pub fn set_root(&self, root: NodeId) {
        self.check(root);
        self.tree.borrow_mut().root = Some(root);
    }

    /// A node's children, in order.
    pub fn children(&self, node: NodeId) -> Vec<NodeId> {
        self.tree
            .borrow()
            .nodes
            .get(&node)
            .map(|node| node.children.clone())
            .unwrap_or_default()
    }

    /// A text node's content, or an element's `None`.
    pub fn text(&self, node: NodeId) -> Option<String> {
        let tree = self.tree.borrow();
        let node = tree.nodes.get(&node)?;
        (node.kind == Some(StubKind::Text)).then(|| node.text.clone())
    }

    /// An element's name, or a text node's or a marker's `None`.
    ///
    /// What a component *built* is part of what it did: an icon that is a `<box>` with a class on
    /// it and an icon that is a `<vector>` carry the same classes, the same attributes and the same
    /// semantics, and only one of them is drawn by the path renderer. A test with no way to ask
    /// this can only assert the half that is the same either way.
    pub fn element_name(&self, node: NodeId) -> Option<String> {
        let tree = self.tree.borrow();
        let node = tree.nodes.get(&node)?;
        node.name.map(|name| name.as_str().to_owned())
    }

    /// An element's class list.
    pub fn classes(&self, node: NodeId) -> Vec<ClassName> {
        self.tree
            .borrow()
            .nodes
            .get(&node)
            .map(|node| node.classes.clone())
            .unwrap_or_default()
    }

    /// One attribute's value, when it is set.
    pub fn attribute(&self, node: NodeId, name: AttrName) -> Option<String> {
        self.tree
            .borrow()
            .nodes
            .get(&node)?
            .attributes
            .get(&name)
            .cloned()
    }

    /// One inline style declaration's value, when it is set.
    pub fn style_property(&self, node: NodeId, property: &str) -> Option<String> {
        self.tree
            .borrow()
            .nodes
            .get(&node)?
            .style_properties
            .get(property)
            .cloned()
    }

    /// One custom property's value, when it is set.
    pub fn custom_property(&self, node: NodeId, property: CustomPropertyName) -> Option<String> {
        self.tree
            .borrow()
            .nodes
            .get(&node)?
            .custom_properties
            .get(&property)
            .cloned()
    }

    /// The interaction states a view has asserted about this element.
    pub fn ui_state(&self, node: NodeId) -> UiState {
        self.tree
            .borrow()
            .nodes
            .get(&node)
            .map_or(UiState::EMPTY, |node| node.ui_state)
    }

    /// Whether an author-defined state is set on this element.
    pub fn has_custom_state(&self, node: NodeId, name: zgui_interned::Ident) -> bool {
        self.tree
            .borrow()
            .nodes
            .get(&node)
            .is_some_and(|node| node.custom_states.contains(&name))
    }

    /// What this element means to an accessibility tree.
    pub fn semantics(&self, node: NodeId) -> Option<Semantics> {
        self.tree.borrow().nodes.get(&node)?.semantics.clone()
    }

    /// One imperative property's value, when it is set.
    pub fn property(&self, node: NodeId, key: PropKey) -> Option<PropValue> {
        self.tree
            .borrow()
            .nodes
            .get(&node)?
            .properties
            .get(&key)
            .cloned()
    }

    /// How many nodes the tree holds.
    pub fn node_count(&self) -> usize {
        self.tree.borrow().nodes.len()
    }

    /// How many listeners are registered across the whole tree.
    pub fn listener_count(&self) -> usize {
        self.tree
            .borrow()
            .nodes
            .values()
            .map(|node| node.listeners.len())
            .sum()
    }

    /// How many observations are live.
    pub fn observation_count(&self) -> usize {
        self.tree.borrow().observations.len()
    }

    /// The text of this node and every node under it, concatenated in order.
    ///
    /// The cheapest way for a test to say "the view says what it should say".
    pub fn text_content(&self, node: NodeId) -> String {
        let mut out = String::new();
        self.append_text(node, &mut out);
        out
    }

    /// The body of [`StubDom::text_content`].
    fn append_text(&self, node: NodeId, out: &mut String) {
        let (kind, text, children) = {
            let tree = self.tree.borrow();
            match tree.nodes.get(&node) {
                Some(node) => (node.kind, node.text.clone(), node.children.clone()),
                None => return,
            }
        };
        if kind == Some(StubKind::Text) {
            out.push_str(&text);
        }
        for child in children {
            self.append_text(child, out);
        }
    }

    /// Delivers `value` to every sink observing `what` on `node`.
    ///
    /// This is the half a real engine performs from its own geometry pass; here it is a method so
    /// that a test can say "layout said the box is this" and watch the view react.
    pub fn deliver(&self, node: NodeId, value: ObservedValue) {
        let sinks: Vec<ObservationSink> = self
            .tree
            .borrow()
            .observations
            .values()
            .filter(|(observed_node, what, _)| *observed_node == node && *what == value.observed())
            .map(|(_, _, sink)| Rc::clone(sink))
            .collect();
        for sink in sinks {
            sink(value);
        }
    }

    /// Mints a handle and stores `node` under it.
    fn add(&self, node: StubNode) -> NodeId {
        let mut tree = self.tree.borrow_mut();
        let bits = tree.next;
        tree.next += 1;
        let id = NodeId::new(self.document, bits).expect("the stub never runs out of handles");
        tree.nodes.insert(id, node);
        id
    }

    /// Panics when `node` was minted by another document.
    fn check(&self, node: NodeId) {
        assert!(
            node.belongs_to(self.document),
            "{node:?} belongs to another document than this tree"
        );
    }

    /// Takes `node` out of whatever parent it is under.
    fn unlink(tree: &mut Tree, node: NodeId) {
        let parent = tree.nodes.get(&node).and_then(|node| node.parent);
        if let Some(parent) = parent
            && let Some(record) = tree.nodes.get_mut(&parent)
        {
            record.children.retain(|child| *child != node);
        }
        if let Some(record) = tree.nodes.get_mut(&node) {
            record.parent = None;
        }
    }
}

impl Dom for StubDom {
    fn create_element(&self, name: ElementName) -> NodeId {
        let mut node = StubNode::of_kind(StubKind::Element);
        node.name = Some(name);
        self.add(node)
    }

    fn create_text(&self, data: &str) -> NodeId {
        let mut node = StubNode::of_kind(StubKind::Text);
        node.text = data.to_owned();
        self.add(node)
    }

    fn create_marker(&self) -> NodeId {
        self.add(StubNode::of_kind(StubKind::Marker))
    }

    fn set_text(&self, node: NodeId, data: &str) {
        self.check(node);
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&node) {
            record.text.clear();
            record.text.push_str(data);
        }
    }

    fn insert(&self, parent: NodeId, child: NodeId, before: Option<NodeId>) {
        self.check(parent);
        self.check(child);
        let mut tree = self.tree.borrow_mut();
        Self::unlink(&mut tree, child);
        let at = match before {
            Some(before) => tree
                .nodes
                .get(&parent)
                .and_then(|record| record.children.iter().position(|node| *node == before))
                .unwrap_or_else(|| {
                    panic!("{before:?} is not a child of {parent:?}, so nothing can go before it")
                }),
            None => tree
                .nodes
                .get(&parent)
                .map_or(0, |record| record.children.len()),
        };
        if let Some(record) = tree.nodes.get_mut(&parent) {
            record.children.insert(at, child);
        }
        if let Some(record) = tree.nodes.get_mut(&child) {
            record.parent = Some(parent);
        }
    }

    fn detach(&self, node: NodeId) {
        self.check(node);
        Self::unlink(&mut self.tree.borrow_mut(), node);
    }

    fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.tree.borrow().nodes.get(&node)?.parent
    }

    fn set_attribute(&self, el: NodeId, name: AttrName, value: Option<&str>) {
        self.check(el);
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&el) {
            match value {
                Some(value) => {
                    record.attributes.insert(name, value.to_owned());
                }
                None => {
                    record.attributes.remove(&name);
                }
            }
        }
    }

    fn set_classes(&self, el: NodeId, classes: &[ClassName]) {
        self.check(el);
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&el) {
            record.classes = classes.to_vec();
        }
    }

    fn toggle_class(&self, el: NodeId, class: ClassName, on: bool) {
        self.check(el);
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&el) {
            let present = record.classes.iter().position(|name| *name == class);
            match (present, on) {
                (None, true) => record.classes.push(class),
                (Some(at), false) => {
                    record.classes.remove(at);
                }
                _ => {}
            }
        }
    }

    fn set_style_text(&self, el: NodeId, css: Option<&str>) {
        self.check(el);
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&el) {
            record.style_text = css.map(str::to_owned);
        }
    }

    fn set_style_property(&self, el: NodeId, property: &str, value: Option<&str>) {
        self.check(el);
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&el) {
            match value {
                Some(value) => {
                    record
                        .style_properties
                        .insert(property.to_owned(), value.to_owned());
                }
                None => {
                    record.style_properties.remove(property);
                }
            }
        }
    }

    fn set_custom_property(&self, el: NodeId, property: CustomPropertyName, value: Option<&str>) {
        self.check(el);
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&el) {
            match value {
                Some(value) => {
                    record.custom_properties.insert(property, value.to_owned());
                }
                None => {
                    record.custom_properties.remove(&property);
                }
            }
        }
    }

    fn set_ui_state(&self, el: NodeId, state: UiState, on: bool) {
        self.check(el);
        assert!(
            UiState::AUTHOR_SETTABLE.contains(state),
            "{state:?} is computed by the framework and cannot be asserted by a view"
        );
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&el) {
            record.ui_state = record.ui_state.apply(state, on);
        }
    }

    fn set_custom_state(&self, el: NodeId, name: zgui_interned::Ident, on: bool) {
        self.check(el);
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&el) {
            if on {
                record.custom_states.insert(name);
            } else {
                record.custom_states.remove(&name);
            }
        }
    }

    fn set_property(&self, el: NodeId, property: PropKey, value: PropValue) {
        self.check(el);
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&el) {
            if value.is_unset() {
                record.properties.remove(&property);
            } else {
                record.properties.insert(property, value);
            }
        }
    }

    fn set_semantics(&self, el: NodeId, semantics: Option<&Semantics>) {
        self.check(el);
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&el) {
            record.semantics = semantics.cloned();
        }
    }

    fn add_listener(
        &self,
        el: NodeId,
        event: EventKind,
        options: ListenerOptions,
        handler: Rc<dyn Fn(&mut EventCx<'_>)>,
    ) -> ListenerId {
        self.check(el);
        let mut tree = self.tree.borrow_mut();
        let id = ListenerId::new(tree.next_listener);
        tree.next_listener += 1;
        if let Some(record) = tree.nodes.get_mut(&el) {
            record.listeners.push(StubListener {
                id,
                event,
                options,
                handler,
            });
        }
        id
    }

    fn remove_listener(&self, el: NodeId, listener: ListenerId) {
        self.check(el);
        if let Some(record) = self.tree.borrow_mut().nodes.get_mut(&el) {
            record
                .listeners
                .retain(|registered| registered.id != listener);
        }
    }

    fn overlay_root(&self, of: NodeId, layer: OverlayLayer) -> NodeId {
        self.check(of);
        if let Some(existing) = self.tree.borrow().overlays.get(&layer) {
            return *existing;
        }
        let band = self.create_element(ElementName::new("overlay_root"));
        self.set_attribute(band, AttrName::new("data-layer"), Some(layer.name()));
        self.tree.borrow_mut().overlays.insert(layer, band);
        // Under the window's root, which is where a real engine keeps it. A band that hung off
        // nothing would be a band no event ever travels through: a press or an Escape is routed
        // from the root down the target's ancestors, so a dismissable surface portalled onto a
        // detached band would never be told about either — and every test of that would pass by
        // never delivering anything.
        let window = self.root(of);
        self.insert(window, band, None);
        band
    }

    fn root(&self, of: NodeId) -> NodeId {
        self.check(of);
        if let Some(existing) = self.tree.borrow().root {
            return existing;
        }
        let root = self.create_element(ElementName::new("root"));
        self.tree.borrow_mut().root = Some(root);
        root
    }

    fn text_content(&self, node: NodeId) -> String {
        Self::text_content(self, node)
    }

    fn observe(&self, node: NodeId, what: Observed, sink: ObservationSink) -> ObservationHandle {
        self.check(node);
        let key = {
            let mut tree = self.tree.borrow_mut();
            let key = tree.next_observation;
            tree.next_observation += 1;
            tree.observations.insert(key, (node, what, sink));
            key
        };
        // The handle holds the tree weakly: a view outliving its backend must deregister into
        // nothing rather than resurrect it.
        let tree = Rc::downgrade(&self.tree);
        ObservationHandle::new(move || {
            if let Some(tree) = tree.upgrade() {
                tree.borrow_mut().observations.remove(&key);
            }
        })
    }
}
