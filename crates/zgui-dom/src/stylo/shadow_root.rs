//! The shadow-root trait, satisfied and unreachable.
//!
//! This document has no shadow trees, so nothing ever produces a shadow root and no method here is
//! ever called. The trait still has to be satisfied, because the node trait names a concrete
//! shadow-root type and there is no way to say "none".
//!
//! The consequence is worth stating where an author can find it: `:host`, `::slotted()` and
//! `::part()` all *parse*, and none of them can ever match, because matching them needs a host
//! element and a shadow-scoped rule set that this returns nothing for. Scoping a component's styles
//! is done with attribute selectors instead. If shadow trees are ever added, this file is the one
//! implementation that has to become real.

use style::dom::TShadowRoot;
use style::stylist::CascadeData;

use crate::node::handle::Node;

impl<'doc> TShadowRoot for Node<'doc> {
    type ConcreteNode = Node<'doc>;

    fn as_node(&self) -> Self::ConcreteNode {
        *self
    }

    /// # Panics
    ///
    /// Always. A shadow root is the only thing that has a host, and nothing here produces one, so
    /// reaching this means a caller invented a shadow root — which is a bug worth stopping for
    /// rather than a case worth answering.
    fn host(&self) -> Node<'doc> {
        unreachable!("this document has no shadow trees, so nothing has a shadow host")
    }

    /// No shadow-scoped rules, because there is no shadow tree to scope them to.
    fn style_data<'a>(&self) -> Option<&'a CascadeData>
    where
        Self: 'a,
    {
        None
    }
}
