//! Attribute lookup, for the two callers that ask by name.
//!
//! Selector matching asks by name and compares the value itself; a container query asks by name and
//! wants the value as a string it can parse. Both go through the one lookup here, so an element's
//! attributes cannot answer one of them and not the other.
//!
//! `id` and `class` are not in the table this reads. They live in the node record — as a copyable
//! identifier handle and as a span into the document's class pool — because matching asks about
//! them far more often than about anything else, and neither answer should cost a column lookup.
//! Asking for either by name here therefore answers [`None`], which is correct for every caller
//! that exists: the matcher never routes `#id` or `.class` through an attribute lookup.

use style::dom::AttributeProvider;
use zgui_vocab::SharedString;

use crate::node::handle::Node;
use crate::side::attrs::Attr;

impl<'doc> Node<'doc> {
    /// This node's attributes other than `id` and `class`, in the order they were set.
    pub fn attrs(self) -> impl Iterator<Item = &'doc Attr> {
        self.store()
            .columns()
            .attrs
            .get(self.key())
            .and_then(Option::as_deref)
            .into_iter()
            .flat_map(crate::side::attrs::AttrMap::iter)
    }

    /// The value of the attribute called `name`, matched as text.
    pub fn attr(self, name: &str) -> Option<&'doc SharedString> {
        self.store()
            .columns()
            .attrs
            .get(self.key())
            .and_then(Option::as_deref)
            .and_then(|attrs| attrs.get_by_str(name))
    }
}

impl AttributeProvider for Node<'_> {
    /// The value of one attribute, as an owned string.
    ///
    /// The namespace is ignored because attributes in this document are unqualified: an attribute
    /// name is written once, by whoever set it, and there is no syntax anywhere above this crate
    /// that produces a prefixed one.
    fn get_attr(&self, attr: &style::LocalName, _namespace: &style::Namespace) -> Option<String> {
        self.attr(&attr.0).map(|value| value.as_str().to_owned())
    }
}
