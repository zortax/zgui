//! What a component is handed as its content.

use std::rc::Rc;

use crate::view::any::AnyView;
use crate::view::view::IntoView;

/// A component's content, built once.
///
/// The ordinary case: a component that puts its children somewhere and never asks for them again.
///
/// ```
/// use zgui_view::{AnyView, Children};
///
/// let children = Children::new(|| AnyView::new("inside"));
/// let view = children.into_view_once();
/// assert!(view.view_type() == AnyView::new("inside").view_type());
/// ```
pub struct Children(Box<dyn FnOnce() -> AnyView>);

impl Children {
    /// Wraps a builder.
    pub fn new(build: impl FnOnce() -> AnyView + 'static) -> Self {
        Self(Box::new(build))
    }

    /// Wraps a view that is already built.
    pub fn from_view(view: impl IntoView + 'static) -> Self {
        let view = AnyView::new(view);
        Self(Box::new(move || view))
    }

    /// Builds the content.
    pub fn into_view_once(self) -> AnyView {
        (self.0)()
    }
}

/// A component's content, which can be built more than once.
///
/// What a component that shows its children in two places, or that shows them again after a
/// branch was taken away, has to be handed instead — a `FnOnce` cannot answer twice.
///
/// ```
/// use zgui_view::{AnyView, ChildrenFn};
///
/// let children = ChildrenFn::new(|| AnyView::new("inside"));
/// let first = children.view();
/// let second = children.view();
/// assert_eq!(first.view_type(), second.view_type());
/// ```
#[derive(Clone)]
pub struct ChildrenFn(Rc<dyn Fn() -> AnyView>);

impl ChildrenFn {
    /// Wraps a builder.
    pub fn new(build: impl Fn() -> AnyView + 'static) -> Self {
        Self(Rc::new(build))
    }

    /// Builds the content.
    pub fn view(&self) -> AnyView {
        (self.0)()
    }
}

impl From<ChildrenFn> for Children {
    fn from(children: ChildrenFn) -> Self {
        Children::new(move || children.view())
    }
}
