//! One node of the in-memory tree.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use zgui_interned::{AttrName, ClassName, CustomPropertyName};
use zgui_vocab::{EventKind, ListenerOptions, PropKey, PropValue, Semantics, UiState};

use crate::dom::ListenerId;
use crate::event::EventCx;
use crate::id::NodeId;

/// What kind of node this is.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum StubKind {
    /// An element, which takes part in selector matching.
    Element,
    /// A text node.
    Text,
    /// A position marker, which takes part in sibling order and nothing else.
    Marker,
}

/// One registered listener.
#[derive(Clone)]
pub struct StubListener {
    /// Which registration this is.
    pub id: ListenerId,
    /// Which event it is registered for.
    pub event: EventKind,
    /// How it was registered.
    pub options: ListenerOptions,
    /// What runs.
    pub handler: Rc<dyn Fn(&mut EventCx<'_>)>,
}

/// One node of the in-memory tree.
#[derive(Default)]
pub struct StubNode {
    /// What kind of node this is.
    pub kind: Option<StubKind>,
    /// The element's name, for an element.
    pub name: Option<zgui_interned::ElementName>,
    /// The node it sits under.
    pub parent: Option<NodeId>,
    /// Its children, in order.
    pub children: Vec<NodeId>,
    /// The text, for a text node.
    pub text: String,
    /// The class list.
    pub classes: Vec<ClassName>,
    /// The attributes that are set.
    pub attributes: BTreeMap<AttrName, String>,
    /// The inline style text, when one is set.
    pub style_text: Option<String>,
    /// The inline style declarations that are set.
    pub style_properties: BTreeMap<String, String>,
    /// The custom properties that are set.
    pub custom_properties: BTreeMap<CustomPropertyName, String>,
    /// The interaction states a view has asserted.
    pub ui_state: UiState,
    /// The author-defined states a view has set.
    pub custom_states: BTreeSet<zgui_interned::Ident>,
    /// The imperative properties that are set.
    pub properties: BTreeMap<PropKey, PropValue>,
    /// What this node means to an accessibility tree.
    pub semantics: Option<Semantics>,
    /// The listeners registered on it.
    pub listeners: Vec<StubListener>,
}

impl StubNode {
    /// A node of the given kind, with nothing else set.
    pub fn of_kind(kind: StubKind) -> Self {
        Self {
            kind: Some(kind),
            ui_state: UiState::EMPTY,
            ..Self::default()
        }
    }
}
