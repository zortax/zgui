//! The node-tree seam.
//!
//! Everything a view does to a tree of nodes goes through [`Dom`], and nothing in this crate does
//! anything to a tree any other way. That is what makes the view layer portable: a native
//! document, a browser's own nodes and a transcript recorder are all "a `Dom`", and no view type,
//! no attribute, no component and no error message mentions which one is installed.

mod handle;
mod listener_id;
mod observe;
mod overlay;

use std::rc::Rc;

use zgui_interned::{AttrName, ClassName, CustomPropertyName, ElementName, Ident};
use zgui_vocab::{EventKind, ListenerOptions, PropKey, PropValue, Semantics, UiState};

use crate::event::EventCx;
use crate::id::NodeId;

pub use crate::dom::handle::DomHandle;
pub use crate::dom::listener_id::ListenerId;
pub use crate::dom::observe::{ObservationHandle, ObservationSink, Observed, ObservedValue};
pub use crate::dom::overlay::OverlayLayer;

/// The node-tree operations the view layer needs.
///
/// Implement this over whatever tree is being driven. The operations are deliberately small and
/// deliberately imperative — create a node, put it somewhere, change one thing about it — because
/// that is the set a retained view layer issues, roughly ten calls per *changed* node per frame,
/// and it is small enough that a new backend is an afternoon rather than a project.
///
/// # What an implementation promises
///
/// * Every [`NodeId`] it returns carries its own [`DocumentId`](crate::DocumentId), and a handle
///   from another document is a programming error it may assert on in debug builds.
/// * [`Dom::insert`] moves a node that is already in the tree rather than duplicating it.
/// * [`Dom::detach`] removes a node from its parent and keeps it usable, because a view detaches
///   and reattaches subtrees as its content changes.
/// * A setter with `None` removes the thing rather than setting it to an empty value.
///
/// # A minimal implementation
///
/// ```
/// use std::rc::Rc;
/// use zgui_view::stub::StubDom;
/// use zgui_view::{Dom, DocumentId};
/// use zgui_interned::{ClassName, ElementName};
///
/// let dom: Rc<dyn Dom> = Rc::new(StubDom::new(DocumentId::FIRST));
/// let row = dom.create_element(ElementName::new("row"));
/// let text = dom.create_text("hello");
/// dom.insert(row, text, None);
/// dom.set_classes(row, &[ClassName::new("toolbar")]);
///
/// assert_eq!(dom.parent(text), Some(row));
/// ```
pub trait Dom {
    /// Creates an element named `name`, detached from any parent.
    fn create_element(&self, name: ElementName) -> NodeId;

    /// Creates a text node holding `data`, detached from any parent.
    fn create_text(&self, data: &str) -> NodeId;

    /// Creates an invisible position marker that holds a place for dynamic content.
    ///
    /// A marker takes part in sibling order, so a view can always say *where* to put content it
    /// has not created yet, and takes no part in element order, so a marker never shifts what a
    /// positional selector matches.
    fn create_marker(&self) -> NodeId;

    /// Replaces a text node's content.
    fn set_text(&self, node: NodeId, data: &str);

    /// Puts `child` under `parent`, immediately before `before`, or last when `before` is `None`.
    fn insert(&self, parent: NodeId, child: NodeId, before: Option<NodeId>);

    /// Removes `node` from its parent, leaving it usable.
    fn detach(&self, node: NodeId);

    /// The node `node` currently sits under.
    fn parent(&self, node: NodeId) -> Option<NodeId>;

    /// Sets or removes one attribute, which is visible to selector matching.
    fn set_attribute(&self, el: NodeId, name: AttrName, value: Option<&str>);

    /// Replaces the whole class list.
    fn set_classes(&self, el: NodeId, classes: &[ClassName]);

    /// Adds or removes one class, leaving the rest of the list alone.
    fn toggle_class(&self, el: NodeId, class: ClassName, on: bool);

    /// Sets or removes the whole inline style text.
    fn set_style_text(&self, el: NodeId, css: Option<&str>);

    /// Sets or removes one inline style declaration.
    ///
    /// `property` is a style property's name as it is written in a style sheet. It is a string
    /// rather than an interned name because the backend has to resolve it against its own style
    /// engine's property table whatever shape it arrives in, and an unknown name is dropped with
    /// a diagnostic rather than rejected here.
    fn set_style_property(&self, el: NodeId, property: &str, value: Option<&str>);

    /// Sets or removes one custom property on this element.
    fn set_custom_property(&self, el: NodeId, property: CustomPropertyName, value: Option<&str>);

    /// Turns one interaction state on or off.
    ///
    /// Only the states a view is allowed to assert reach this: the control states that mirror an
    /// accessibility property. Hover, focus and activation are computed by the input system and a
    /// view that could assert them would be lying to it.
    fn set_ui_state(&self, el: NodeId, state: UiState, on: bool);

    /// Turns one author-defined state on or off.
    ///
    /// An author-defined state is matched by `:state(name)` and is how a view expresses a
    /// condition the closed set of interaction states does not name — that a list item is
    /// selected, that a step is complete.
    fn set_custom_state(&self, el: NodeId, name: Ident, on: bool);

    /// Sets one imperative property, which is neither an attribute nor visible to selectors.
    fn set_property(&self, el: NodeId, property: PropKey, value: PropValue);

    /// Replaces, or removes, what this element means to an accessibility tree.
    fn set_semantics(&self, el: NodeId, semantics: Option<&Semantics>);

    /// Registers `handler` for `event` on `el`, returning the registration's name.
    fn add_listener(
        &self,
        el: NodeId,
        event: EventKind,
        options: ListenerOptions,
        handler: Rc<dyn Fn(&mut EventCx<'_>)>,
    ) -> ListenerId;

    /// Removes a registration [`Dom::add_listener`] returned.
    fn remove_listener(&self, el: NodeId, listener: ListenerId);

    /// The overlay root for portalled content on `layer`, in the window `of` belongs to.
    ///
    /// A backend with no native popup surface answers with the window's own overlay root for that
    /// band, which is what keeps a portal expressible everywhere.
    fn overlay_root(&self, of: NodeId, layer: OverlayLayer) -> NodeId;

    /// The root element of the window `of` belongs to.
    ///
    /// A view can attach a listener only to a node it created, so without this there is no way to
    /// hear about a press somewhere else in the document — which is exactly what dismissing an
    /// open menu by clicking past it requires.
    fn root(&self, of: NodeId) -> NodeId;

    /// Every character `node`'s subtree contributes, in order.
    ///
    /// What a composite control matches a typed character against. A menu's typeahead, a
    /// listbox's, and a select's all answer the same question — *which item reads as starting
    /// with this?* — and the text an item reads as is the text it renders, which is written by
    /// whoever wrote the item rather than declared to the control as a separate string.
    ///
    /// Markers and elements contribute nothing of their own; the answer is the concatenation of
    /// the text nodes below `node`, and it is empty for a node that has none.
    fn text_content(&self, node: NodeId) -> String;

    /// Observes one geometric quantity of `node`, delivering each new value to `sink`.
    ///
    /// The value is delivered after layout has settled and before anything is painted, so a view
    /// that repositions itself from what it observes is painted in its final place in the same
    /// frame rather than one frame late. Dropping the returned handle deregisters.
    fn observe(&self, node: NodeId, what: Observed, sink: ObservationSink) -> ObservationHandle;
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use zgui_interned::ElementName;

    use super::Dom;
    use crate::DocumentId;
    use crate::stub::StubDom;

    #[test]
    fn the_trait_is_object_safe() {
        let dom: Rc<dyn Dom> = Rc::new(StubDom::new(DocumentId::FIRST));
        let node = dom.create_element(ElementName::new("box"));
        assert!(node.belongs_to(DocumentId::FIRST));
    }
}
