//! The transcript backend.

use std::rc::Rc;

use zgui_interned::{AttrName, ClassName, CustomPropertyName, ElementName, Ident};
use zgui_view::stub::StubDom;
use zgui_view::{
    DocumentId, Dom, EventCx, ListenerId, NodeId, ObservationHandle, ObservationSink, Observed,
    ObservedValue, OverlayLayer,
};
use zgui_vocab::{EventKind, ListenerOptions, PropKey, PropValue, Semantics, UiState};

use crate::dom::editing::Editing;
use crate::dom::handlers::{Handlers, Registration};
use crate::transcript::{Op, Transcript};

/// A [`Dom`] that keeps a real tree and writes down every change made to it.
///
/// The tree underneath is the view layer's own in-memory one, so this records rather than
/// reimplements: a component test asserting on the transcript and a component test asking the tree
/// what it holds cannot disagree, because there is one tree.
///
/// ```
/// use zgui_interned::ElementName;
/// use zgui_testkit_view::RecordingDom;
/// use zgui_view::{DocumentId, Dom};
///
/// let dom = RecordingDom::new(DocumentId::FIRST);
/// let row = dom.create_element(ElementName::new("row"));
/// let text = dom.create_text("hello");
/// dom.insert(row, text, None);
///
/// assert_eq!(dom.parent(text), Some(row));
/// assert_eq!(
///     dom.transcript().to_string(),
///     "create #1 row\ncreate #2 #text\ntext #2 \"hello\"\ninsert #2 into #1\n"
/// );
/// ```
pub struct RecordingDom {
    /// The tree.
    tree: StubDom,
    /// What has been asked of it.
    transcript: Transcript,
    /// The handlers, which the tree does not hold.
    handlers: Handlers,
    /// The editing models over whatever in this tree can be typed into.
    ///
    /// Beside the tree rather than in it, exactly as a window keeps them beside its document: a
    /// model holds the caret, the undo stack and any composition, none of which can be recovered
    /// from the text, so it has to outlive the keystroke that made it.
    editing: Editing,
}

impl RecordingDom {
    /// An empty tree belonging to `document`, with an empty transcript.
    pub fn new(document: DocumentId) -> Self {
        Self::with_transcript(document, Transcript::new())
    }

    /// The same, recording into a transcript something else is also recording into.
    pub fn with_transcript(document: DocumentId, transcript: Transcript) -> Self {
        Self {
            tree: StubDom::new(document),
            transcript,
            handlers: Handlers::new(),
            editing: Editing::default(),
        }
    }

    /// What has been asked of this tree.
    pub fn transcript(&self) -> Transcript {
        self.transcript.clone()
    }

    /// The handlers registered on it.
    pub fn handlers(&self) -> Handlers {
        self.handlers.clone()
    }

    /// The tree itself, for the questions a transcript cannot answer.
    pub fn tree(&self) -> &StubDom {
        &self.tree
    }

    /// The editing models over whatever in this tree can be typed into.
    pub(crate) fn editing(&self) -> &Editing {
        &self.editing
    }

    /// Declares which node is this window's root, which is what [`Dom::root`] then answers.
    ///
    /// A harness that made a root of its own and mounted everything under it declares it here.
    /// Left undeclared, the first caller to ask for the root mints a second one, and a view
    /// listening on "the window's root" would be listening on an element nothing is under.
    pub fn set_root(&self, root: NodeId) {
        self.tree.set_root(root);
    }

    /// Which document this tree is.
    pub fn document(&self) -> DocumentId {
        self.tree.document()
    }

    /// Delivers an observed value to whatever registered for it.
    pub fn deliver(&self, node: NodeId, value: ObservedValue) {
        self.tree.deliver(node, value);
    }

    /// Runs `mint`, and records a creation when it turned out to make a node.
    ///
    /// The two lazy roots — an overlay layer's and the window's — are asked for by name rather than
    /// created, and the first caller to ask is the one that brings the node into being. Without
    /// this the transcript would name a node it never recorded creating, which is the one thing a
    /// transcript may not do: every golden of every overlay would insert into a number that
    /// appears from nowhere.
    fn recording_creation(&self, what: &str, mint: impl FnOnce() -> NodeId) -> NodeId {
        let before = self.tree.node_count();
        let node = mint();
        if self.tree.node_count() != before {
            self.transcript.push(Op::Create {
                node,
                what: what.to_owned(),
            });
        }
        node
    }
}

impl core::fmt::Debug for RecordingDom {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("RecordingDom")
            .field("nodes", &self.tree.node_count())
            .field("recorded", &self.transcript.len())
            .finish()
    }
}

impl Dom for RecordingDom {
    fn create_element(&self, name: ElementName) -> NodeId {
        let node = self.tree.create_element(name);
        self.transcript.push(Op::Create {
            node,
            what: name.as_str().to_owned(),
        });
        node
    }

    fn create_text(&self, data: &str) -> NodeId {
        let node = self.tree.create_text(data);
        self.transcript.push(Op::Create {
            node,
            what: "#text".to_owned(),
        });
        if !data.is_empty() {
            self.transcript.push(Op::SetText {
                node,
                text: data.to_owned(),
            });
        }
        node
    }

    fn create_marker(&self) -> NodeId {
        let node = self.tree.create_marker();
        self.transcript.push(Op::Create {
            node,
            what: "#marker".to_owned(),
        });
        node
    }

    fn set_text(&self, node: NodeId, data: &str) {
        self.tree.set_text(node, data);
        self.transcript.push(Op::SetText {
            node,
            text: data.to_owned(),
        });
    }

    fn insert(&self, parent: NodeId, child: NodeId, before: Option<NodeId>) {
        self.tree.insert(parent, child, before);
        self.transcript.push(Op::Insert {
            parent,
            child,
            before,
        });
    }

    fn detach(&self, node: NodeId) {
        self.tree.detach(node);
        self.transcript.push(Op::Detach { node });
    }

    fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.tree.parent(node)
    }

    fn set_attribute(&self, el: NodeId, name: AttrName, value: Option<&str>) {
        self.tree.set_attribute(el, name, value);
        self.transcript.push(Op::SetAttribute {
            node: el,
            name: name.as_str().to_owned(),
            value: value.map(str::to_owned),
        });
    }

    fn set_classes(&self, el: NodeId, classes: &[ClassName]) {
        self.tree.set_classes(el, classes);
        self.transcript.push(Op::SetClasses {
            node: el,
            classes: classes
                .iter()
                .map(|class| class.as_str().to_owned())
                .collect(),
        });
    }

    fn toggle_class(&self, el: NodeId, class: ClassName, on: bool) {
        self.tree.toggle_class(el, class, on);
        self.transcript.push(Op::ToggleClass {
            node: el,
            class: class.as_str().to_owned(),
            on,
        });
    }

    fn set_style_text(&self, el: NodeId, css: Option<&str>) {
        self.tree.set_style_text(el, css);
        self.transcript.push(Op::SetStyle {
            node: el,
            property: "style".to_owned(),
            value: css.map(str::to_owned),
        });
    }

    fn set_style_property(&self, el: NodeId, property: &str, value: Option<&str>) {
        self.tree.set_style_property(el, property, value);
        self.transcript.push(Op::SetStyle {
            node: el,
            property: property.to_owned(),
            value: value.map(str::to_owned),
        });
    }

    fn set_custom_property(&self, el: NodeId, property: CustomPropertyName, value: Option<&str>) {
        self.tree.set_custom_property(el, property, value);
        self.transcript.push(Op::SetStyle {
            node: el,
            property: property.as_str().to_owned(),
            value: value.map(str::to_owned),
        });
    }

    fn set_ui_state(&self, el: NodeId, state: UiState, on: bool) {
        self.tree.set_ui_state(el, state, on);
        self.transcript.push(Op::SetUiState {
            node: el,
            state: format!("{state:?}"),
            on,
        });
    }

    fn set_custom_state(&self, el: NodeId, name: Ident, on: bool) {
        self.tree.set_custom_state(el, name, on);
        self.transcript.push(Op::SetCustomState {
            node: el,
            name: name.as_str().to_owned(),
            on,
        });
    }

    fn set_property(&self, el: NodeId, property: PropKey, value: PropValue) {
        self.tree.set_property(el, property, value.clone());
        self.transcript.push(Op::SetProperty {
            node: el,
            property: property.as_str().to_owned(),
            value: property_text(&value),
        });
    }

    fn set_semantics(&self, el: NodeId, semantics: Option<&Semantics>) {
        self.tree.set_semantics(el, semantics);
        self.transcript.push(Op::SetSemantics {
            node: el,
            role: semantics.map(|semantics| format!("{:?}", semantics.role)),
        });
    }

    fn add_listener(
        &self,
        el: NodeId,
        event: EventKind,
        options: ListenerOptions,
        handler: Rc<dyn Fn(&mut EventCx<'_>)>,
    ) -> ListenerId {
        let id = self
            .tree
            .add_listener(el, event, options, Rc::clone(&handler));
        self.handlers.add(
            el,
            Registration {
                id,
                event,
                options,
                handler,
            },
        );
        self.transcript.push(Op::AddListener {
            node: el,
            event: event.name().to_owned(),
            capture: options.capture,
        });
        id
    }

    fn remove_listener(&self, el: NodeId, listener: ListenerId) {
        self.tree.remove_listener(el, listener);
        if self.handlers.remove(el, listener) {
            self.transcript.push(Op::RemoveListener { node: el });
        }
    }

    fn overlay_root(&self, of: NodeId, layer: OverlayLayer) -> NodeId {
        // A band hangs off the window's root, so asking for one mints that root as well. It is
        // minted through this method rather than left to the tree, because a node the transcript
        // never saw created is a number that appears from nowhere in every golden after it.
        let _ = self.root(of);
        self.recording_creation("overlay_root", || self.tree.overlay_root(of, layer))
    }

    fn root(&self, of: NodeId) -> NodeId {
        self.recording_creation("root", || self.tree.root(of))
    }

    fn text_content(&self, node: NodeId) -> String {
        self.tree.text_content(node)
    }

    fn observe(&self, node: NodeId, what: Observed, sink: ObservationSink) -> ObservationHandle {
        self.transcript.push(Op::Observe {
            node,
            what: format!("{what:?}"),
        });
        self.tree.observe(node, what, sink)
    }
}

/// One imperative property's value, as a transcript writes it.
///
/// Spelled out here rather than taken from the value's own formatting, because a transcript is
/// compared against a checked-in file: a derived rendering that gained a field would rewrite every
/// golden that had ever recorded a property.
fn property_text(value: &PropValue) -> Option<String> {
    match value {
        PropValue::Unset => None,
        PropValue::Bool(flag) => Some(flag.to_string()),
        PropValue::Integer(number) => Some(number.to_string()),
        PropValue::Number(number) => Some(number.to_string()),
        PropValue::Text(text) => Some(text.as_str().to_owned()),
        _ => Some(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use zgui_interned::{ClassName, ElementName};
    use zgui_view::{DocumentId, Dom, ListenerOptions};
    use zgui_vocab::EventKind;

    use super::RecordingDom;
    use crate::transcript::Op;

    #[test]
    fn the_tree_and_the_transcript_cannot_disagree() {
        let dom = RecordingDom::new(DocumentId::FIRST);
        let row = dom.create_element(ElementName::new("row"));
        dom.toggle_class(row, ClassName::new("busy"), true);

        assert_eq!(dom.tree().classes(row), vec![ClassName::new("busy")]);
        assert!(dom.transcript().ops().contains(&Op::ToggleClass {
            node: row,
            class: "busy".to_owned(),
            on: true,
        }));
    }

    #[test]
    fn a_removed_listener_is_gone_from_the_table_and_from_the_transcript() {
        let dom = RecordingDom::new(DocumentId::FIRST);
        let control = dom.create_element(ElementName::new("control"));
        let ran = Rc::new(Cell::new(0));
        let id = {
            let ran = Rc::clone(&ran);
            dom.add_listener(
                control,
                EventKind::Click,
                ListenerOptions::DEFAULT,
                Rc::new(move |_| ran.set(ran.get() + 1)),
            )
        };
        assert_eq!(dom.handlers().len(), 1);

        dom.remove_listener(control, id);
        assert_eq!(dom.handlers().len(), 0);
        assert!(dom.handlers().of(control, EventKind::Click).is_empty());
        assert!(
            dom.transcript()
                .ops()
                .contains(&Op::RemoveListener { node: control })
        );

        // Removing it a second time is not a second line: a transcript records what changed.
        let before = dom.transcript().len();
        dom.remove_listener(control, id);
        assert_eq!(dom.transcript().len(), before);
    }

    #[test]
    fn the_lazy_roots_are_recorded_as_creations_the_first_time_they_are_asked_for() {
        // An overlay layer's root is asked for by name, and the first caller to ask is the one
        // that makes it. A transcript that skipped that would insert into a node whose number
        // appears from nowhere, which every dialog, popover and tooltip golden would then carry.
        use zgui_view::OverlayLayer;

        let dom = RecordingDom::new(DocumentId::FIRST);
        let anchor = dom.create_element(ElementName::new("control"));
        let overlay = dom.overlay_root(anchor, OverlayLayer::Popover);
        let again = dom.overlay_root(anchor, OverlayLayer::Popover);
        let root = dom.root(anchor);

        assert_eq!(again, overlay, "asking twice makes one");
        assert_eq!(
            dom.transcript().to_string(),
            format!(
                "create #{} control\ncreate #{} root\ncreate #{} overlay_root\n",
                anchor.backend_bits(),
                root.backend_bits(),
                overlay.backend_bits()
            ),
            "the band's own root came with it, and the second ask makes nothing"
        );
        assert_eq!(
            dom.parent(overlay),
            Some(root),
            "a band nothing can be reached through is a band no event travels to"
        );
    }

    #[test]
    fn an_imperative_property_is_recorded_and_clearing_one_says_so() {
        // A field's value travels this way and nothing else in a transcript would show it moving.
        use zgui_vocab::{PropKey, PropValue};

        let dom = RecordingDom::new(DocumentId::FIRST);
        let field = dom.create_element(ElementName::new("field"));
        dom.set_property(field, PropKey::new("value"), PropValue::from("hello"));
        dom.set_property(field, PropKey::new("value"), PropValue::Unset);

        assert_eq!(
            dom.transcript().to_string(),
            format!(
                "create #{0} field\nprop #{0} value=\"hello\"\nprop #{0} value removed\n",
                field.backend_bits()
            )
        );
    }

    #[test]
    fn creating_a_text_node_records_its_content_and_an_empty_one_does_not() {
        let dom = RecordingDom::new(DocumentId::FIRST);
        dom.create_text("hello");
        dom.create_text("");
        assert_eq!(
            dom.transcript().to_string(),
            "create #1 #text\ntext #1 \"hello\"\ncreate #2 #text\n"
        );
    }
}
