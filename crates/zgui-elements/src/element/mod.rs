//! The builder every element name starts, and what building one produces.

mod attribute;
mod children;
mod state;
mod typed;
mod universal;

use core::marker::PhantomData;

use zgui_view::{A11yBinding, AnyView, BuildCx, Classes, View};

use crate::element::attribute::Attribute;
use crate::tag::Tag;

pub use crate::element::state::ElementState;

/// An element being described.
///
/// Every method takes the builder and gives it back, so an element is one expression. Nothing is
/// created while it is being described: the node, its attributes and its children all come into
/// existence when the view is built, which is what lets an element be passed around, stored and
/// returned from a function like any other value.
///
/// Whether an attribute is written once or kept up to date is decided by what is passed to it. A
/// literal is written at build time with no reactive machinery behind it; a signal or a closure
/// gets exactly one binding, which writes only when the value it computes actually changes.
///
/// ```
/// use zgui_elements::{column, text};
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, RwSignal, install};
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::{Anchor, BuildCxOwned, DocumentId, DomHandle, HostHandle, View};
/// use zgui_interned::ElementName;
/// use std::rc::Rc;
///
/// install().unwrap();
/// let backend = Rc::new(StubDom::new(DocumentId::FIRST));
/// let dom = DomHandle::from_rc(backend.clone());
/// let window = Mounted::new();
/// let cx = BuildCxOwned::new(
///     dom.clone(),
///     HostHandle::new(StubHost::default()),
///     window.owner().clone(),
///     DocumentId::FIRST,
/// );
/// let root = dom.create_element(ElementName::new("box"));
///
/// let count = window.with(|| RwSignal::new(0));
/// let mut built = window.with(|| {
///     column()
///         .class("counter")
///         .child(text().child(move || count.get().to_string()))
///         .build(&mut cx.cx())
/// });
/// built.mount(&dom, root, None);
/// assert_eq!(backend.text_content(root), "0");
///
/// count.set(7);
/// zgui_reactive::flush();
/// assert_eq!(backend.text_content(root), "7");
/// window.unmount();
/// ```
pub struct Element<T: Tag> {
    /// What was said about it, in the order it was said.
    attributes: Vec<Attribute>,
    /// The whole class list, merged from every source that contributed one.
    classes: Classes,
    /// What this element means, merged from every accessibility property that was set.
    a11y: Option<A11yBinding>,
    /// The children, in order.
    children: Vec<AnyView>,
    /// Which element this is.
    tag: PhantomData<fn() -> T>,
}

impl<T: Tag> Default for Element<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Tag> Element<T> {
    /// An element with nothing said about it yet.
    pub fn new() -> Self {
        Self {
            attributes: Vec::new(),
            classes: Classes::new(),
            a11y: None,
            children: Vec::new(),
            tag: PhantomData,
        }
    }

    /// Records one thing to do to the element once it exists.
    fn push(mut self, attribute: Attribute) -> Self {
        self.attributes.push(attribute);
        self
    }
}

impl<T: Tag> View for Element<T> {
    type State = ElementState;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let node = cx.dom().create_element(T::name());
        let mut state = ElementState::new(node);
        state.apply(self.attributes, self.classes, self.a11y, cx);
        state.build_children(self.children, cx);
        state
    }

    /// Says the same things again to the element that is already there.
    ///
    /// The node is never re-created — it is the same element, described again — so a rebuild
    /// replaces the bindings and rebuilds each child in place. What an earlier description wrote
    /// and this one does not mention stays written, exactly as it would if the two descriptions
    /// were two separate statements about the same element.
    ///
    /// Listeners are the exception, and they have to be: they are replaced rather than added to.
    /// An element written inside a closure is described again on every change the closure reads,
    /// and a listener kept from each of those descriptions would mean one extra call of the
    /// handler per change.
    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        state.apply(self.attributes, self.classes, self.a11y, cx);
        state.rebuild_children(self.children, cx);
    }
}
