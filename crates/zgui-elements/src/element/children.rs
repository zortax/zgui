//! Putting content inside an element.

use zgui_view::{AnyView, IntoView};

use crate::element::Element;
use crate::tag::Tag;

impl<T: Tag> Element<T> {
    /// Adds one child, after the ones already added.
    ///
    /// Anything that is a view is a child: a string, a number, another element, a component's
    /// return value, a closure that computes one of those — and `Option`, which contributes
    /// nothing when it is `None`.
    #[must_use]
    pub fn child(mut self, child: impl IntoView) -> Self {
        self.children.push(AnyView::new(child));
        self
    }

    /// Adds every child of `children`, in order.
    ///
    /// What a component does with the content its caller wrote between its tags.
    #[must_use]
    pub fn children(mut self, children: impl IntoIterator<Item = AnyView>) -> Self {
        self.children.extend(children);
        self
    }
}
