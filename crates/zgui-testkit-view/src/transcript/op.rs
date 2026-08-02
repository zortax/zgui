//! One line of a transcript.

use zgui_view::NodeId;

/// Something a view asked its backend to do.
///
/// One variant per operation that is worth asserting on, and deliberately not one per method: what
/// a component test wants to know is *what changed*, not how many times a setter was called with
/// the value it already had. Operations that answer a question rather than change anything —
/// reading a parent, reading a border box — are not recorded, because a test that asserted on
/// those would break every time a view layer became cleverer about caching.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Op {
    /// A node was created.
    Create {
        /// The node.
        node: NodeId,
        /// What it is: an element's name, `#text`, or `#marker`.
        what: String,
    },
    /// A node was put under a parent.
    Insert {
        /// The parent.
        parent: NodeId,
        /// The child.
        child: NodeId,
        /// The sibling it went before, if it did not go last.
        before: Option<NodeId>,
    },
    /// A node was taken out of its parent.
    Detach {
        /// The node.
        node: NodeId,
    },
    /// A text node's content was replaced.
    SetText {
        /// The node.
        node: NodeId,
        /// The new text.
        text: String,
    },
    /// One attribute was set or removed.
    SetAttribute {
        /// The element.
        node: NodeId,
        /// The attribute's name.
        name: String,
        /// The new value, or nothing when it was removed.
        value: Option<String>,
    },
    /// The class list was replaced.
    SetClasses {
        /// The element.
        node: NodeId,
        /// The new list, in order.
        classes: Vec<String>,
    },
    /// One class was added or removed.
    ToggleClass {
        /// The element.
        node: NodeId,
        /// The class.
        class: String,
        /// Whether it is on now.
        on: bool,
    },
    /// Something about the element's inline style changed.
    SetStyle {
        /// The element.
        node: NodeId,
        /// Which declaration, or the whole block when it is `style`.
        property: String,
        /// The new value, or nothing when it was removed.
        value: Option<String>,
    },
    /// One interaction state was asserted or withdrawn.
    SetUiState {
        /// The element.
        node: NodeId,
        /// The state's name.
        state: String,
        /// Whether it is on now.
        on: bool,
    },
    /// One imperative property was set or removed.
    ///
    /// Neither an attribute nor anything a selector can see, and recorded all the same: a field's
    /// value and an editor's selection travel this way, so a transcript that left it out could not
    /// show a form control changing at all.
    SetProperty {
        /// The element.
        node: NodeId,
        /// The property's name.
        property: String,
        /// The new value, or nothing when the property was removed.
        value: Option<String>,
    },
    /// One author-defined state was set or cleared.
    SetCustomState {
        /// The element.
        node: NodeId,
        /// The state's name.
        name: String,
        /// Whether it is on now.
        on: bool,
    },
    /// What this element means to an accessibility tree was set or cleared.
    SetSemantics {
        /// The element.
        node: NodeId,
        /// The role, or nothing when the semantics were cleared.
        role: Option<String>,
    },
    /// A listener was registered.
    AddListener {
        /// The element.
        node: NodeId,
        /// Which event.
        event: String,
        /// Which leg it runs in.
        capture: bool,
    },
    /// A listener was removed.
    RemoveListener {
        /// The element.
        node: NodeId,
    },
    /// An observation was registered.
    Observe {
        /// The node being observed.
        node: NodeId,
        /// What about it.
        what: String,
    },
    /// Focus was asked to move.
    Focus {
        /// Where to.
        node: NodeId,
    },
    /// A scroll was asked for.
    Scroll {
        /// Which container.
        node: NodeId,
    },
    /// A focus trap was installed.
    PushFocusTrap {
        /// The subtree it confines traversal to.
        node: NodeId,
    },
    /// A focus trap was removed.
    PopFocusTrap,
    /// A selection was replaced.
    SetSelection {
        /// The editable node.
        node: NodeId,
        /// The new range's start.
        start: usize,
        /// Its end.
        end: usize,
    },
    /// Everything in an editable node was selected.
    SelectAll {
        /// The editable node.
        node: NodeId,
    },
    /// A value was loaded into an editable node.
    ///
    /// Only a load that changed the text is recorded. A controlled field is told its own value back
    /// on every keystroke and the host does nothing with it, so recording those would fill a
    /// transcript with lines describing a window in which nothing happened.
    SetValue {
        /// The editable node.
        node: NodeId,
        /// The text it now holds.
        text: String,
    },
    /// A handler ran.
    ///
    /// Recorded by the scripted input rather than by either backend, which is what puts what a
    /// component *did* and what it was *told* in one order.
    Handler {
        /// The element whose listener ran.
        node: NodeId,
        /// Which event.
        event: String,
        /// Which leg of the delivery.
        phase: String,
    },
    /// A style sheet was installed or replaced.
    InstallStylesheet {
        /// The name it is installed under.
        name: String,
    },
    /// A style sheet was removed.
    RemoveStylesheet {
        /// The name it was installed under.
        name: String,
    },
    /// A handler asked for something through the command sink.
    Command {
        /// What it asked for.
        what: String,
        /// Which node it named.
        node: NodeId,
    },
}
