//! Attribute selectors.
//!
//! One method, and the whole of it is a lookup plus a call into the engine's own comparison. That
//! delegation is not laziness: the operation carries both the operator — presence, equality, prefix,
//! suffix, substring, dash-match, word-match — *and* the case sensitivity the selector's `i` or `s`
//! flag asked for. Comparing the strings by hand loses the flags silently, and a selector written
//! `[data-state="OPEN" i]` would then match nothing while looking correct.
//!
//! Namespaces are ignored because attributes in this document are unqualified: an attribute name is
//! written once, by whoever set it, and nothing above this crate has syntax for a prefixed one.
//!
//! Unreachable on a node that is not an element, and the assertion says so.

use selectors::attr::{AttrSelectorOperation, NamespaceConstraint};
use style::values::AtomString;

use crate::node::handle::Node;
use crate::stylo::selectors::expect_element;

impl Node<'_> {
    /// Whether this element's `local_name` attribute satisfies `operation`.
    pub fn matches_attr(
        self,
        namespace: &NamespaceConstraint<&style::Namespace>,
        local_name: &style::LocalName,
        operation: &AttrSelectorOperation<&AtomString>,
    ) -> bool {
        expect_element(self);
        let _ = namespace;
        self.attr(&local_name.0)
            .is_some_and(|value| operation.eval_str(value.as_str()))
    }
}
