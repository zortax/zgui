//! The document trait: the lock every stylesheet shares, and two policy answers.
//!
//! One optional method is deliberately left unanswered. The engine can ask a document for every
//! element carrying a given identifier, and uses the answer to resolve a scope root without walking
//! the tree; a document that cannot answer says so and the engine walks instead. Answering would
//! mean maintaining an identifier-to-elements index across every insertion, removal and identifier
//! write, and nothing in this document is bounded by that walk today. The index is worth adding
//! when a measurement says so, and until then the honest answer is the one that costs nothing.

use selectors::matching::QuirksMode;
use style::dom::TDocument;
use style::shared_lock::SharedRwLock;

use crate::node::handle::Node;

impl<'doc> TDocument for Node<'doc> {
    type ConcreteNode = Node<'doc>;

    fn as_node(&self) -> Self::ConcreteNode {
        *self
    }

    /// Not an HTML document.
    ///
    /// The answer decides whether tag names and attribute names are matched case-insensitively. A
    /// document with no markup language on top of it has no case-folding rule to inherit, so names
    /// match exactly as written — which is also the only answer under which a component library can
    /// rely on the names it chose.
    fn is_html_document(&self) -> bool {
        false
    }

    /// Never in quirks mode.
    ///
    /// Quirks mode exists to keep documents written for browsers of the nineteen-nineties
    /// rendering, and nothing here can be one.
    fn quirks_mode(&self) -> QuirksMode {
        QuirksMode::NoQuirks
    }

    /// The one lock this document's stylesheets, style attributes and restyle guards all share.
    ///
    /// A declaration block behind a different lock cannot be read under the guard the engine takes
    /// for a restyle, so this being the *document's* lock rather than the engine's is structural:
    /// an engine built first would have to hand its lock to the document instead.
    fn shared_lock(&self) -> &SharedRwLock {
        self.store().lock()
    }
}
