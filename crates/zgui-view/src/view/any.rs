//! A view whose type is not in the signature.

use core::any::TypeId;

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::anchor::{Anchor, AnyAnchor};
use crate::view::hole::Hole;
use crate::view::view::{IntoView, View};

/// The erased half of a view.
trait ErasedView {
    /// Which view this was, so a rebuild can tell whether it is the same one.
    fn view_type(&self) -> TypeId;

    /// Builds it.
    fn build_erased(self: Box<Self>, cx: &mut BuildCx<'_>) -> Box<dyn AnyAnchor>;

    /// Rebuilds it into `state`, which must be the state this view's own `build` produced.
    fn rebuild_erased(self: Box<Self>, state: &mut dyn AnyAnchor, cx: &mut BuildCx<'_>);
}

impl<V: View> ErasedView for V {
    fn view_type(&self) -> TypeId {
        TypeId::of::<V>()
    }

    fn build_erased(self: Box<Self>, cx: &mut BuildCx<'_>) -> Box<dyn AnyAnchor> {
        Box::new((*self).build(cx))
    }

    fn rebuild_erased(self: Box<Self>, state: &mut dyn AnyAnchor, cx: &mut BuildCx<'_>) {
        let state = state
            .as_any_mut()
            .downcast_mut::<V::State>()
            .expect("an erased view is only ever rebuilt into the state its own build produced");
        (*self).rebuild(state, cx);
    }
}

/// A view with its type erased.
///
/// This is what a component returns when two call sites need to produce different views, what a
/// window's root builder hands back, and what [`Dynamic`](crate::Dynamic) switches between. It is
/// one boxed value — there is no parallel trait hierarchy behind it — because the view layer
/// dispatches through a trait object at the backend seam anyway.
///
/// Rebuilding an erased view with a view of the *same* type rebuilds in place, exactly as the
/// typed path does. Rebuilding it with a different type unmounts what was there and builds the
/// new one in its place.
///
/// ```
/// use zgui_reactive::{Mounted, install};
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::{Anchor, AnyView, BuildCxOwned, DocumentId, DomHandle, HostHandle, IntoView, View};
/// use zgui_interned::ElementName;
/// use std::rc::Rc;
///
/// install().unwrap();
/// let backend = Rc::new(StubDom::new(DocumentId::FIRST));
/// let dom = DomHandle::from_rc(backend.clone());
/// let window = Mounted::new();
/// let cx = BuildCxOwned::new(
///     dom.clone(), HostHandle::new(StubHost::default()),
///     window.owner().clone(), DocumentId::FIRST,
/// );
/// let root = dom.create_element(ElementName::new("box"));
///
/// let mut state = AnyView::new("text").build(&mut cx.cx());
/// state.mount(&dom, root, None);
/// assert_eq!(backend.text_content(root), "text");
///
/// AnyView::new(42u32).rebuild(&mut state, &mut cx.cx()); // a different type entirely
/// assert_eq!(backend.text_content(root), "42");
/// ```
pub struct AnyView {
    /// The view.
    inner: Box<dyn ErasedView>,
}

impl AnyView {
    /// Erases `view`.
    pub fn new(view: impl IntoView) -> Self {
        Self {
            inner: Box::new(view.into_view()),
        }
    }

    /// Which view this is.
    pub fn view_type(&self) -> TypeId {
        self.inner.view_type()
    }
}

/// What an erased view retains.
pub struct AnyViewState {
    /// Where the content sits.
    hole: Hole<Box<dyn AnyAnchor>>,
    /// Which view built it.
    view_type: TypeId,
}

impl Anchor for AnyViewState {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        self.hole.mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.hole.unmount(dom);
    }

    fn first_node(&self) -> Option<NodeId> {
        self.hole.first_node()
    }
}

impl View for AnyView {
    type State = AnyViewState;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let view_type = self.inner.view_type();
        let mut hole = Hole::new(cx.dom());
        let built = self.inner.build_erased(cx);
        hole.fill(cx.dom(), built);
        AnyViewState { hole, view_type }
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        let view_type = self.inner.view_type();
        match state.hole.content_mut() {
            Some(existing) if state.view_type == view_type => {
                self.inner.rebuild_erased(&mut **existing, cx);
            }
            _ => {
                let built = self.inner.build_erased(cx);
                state.hole.set(cx.dom(), Some(built));
                state.view_type = view_type;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AnyView;
    use crate::fixture::Fixture;
    use crate::view::anchor::Anchor;
    use crate::view::view::View;

    #[test]
    fn the_same_view_type_rebuilds_in_place() {
        let f = Fixture::new();
        let mut state = AnyView::new("a").build(&mut f.cx());
        state.mount(&f.dom, f.root, None);
        let node = state.first_node();

        AnyView::new("b").rebuild(&mut state, &mut f.cx());
        assert_eq!(f.text(), "b");
        assert_eq!(state.first_node(), node, "the text node was reused");
        f.window.unmount();
    }

    #[test]
    fn a_different_view_type_replaces_the_content_in_the_same_place() {
        let f = Fixture::new();
        let after = f.dom.create_text("|");
        let mut state = AnyView::new("a").build(&mut f.cx());
        state.mount(&f.dom, f.root, None);
        f.dom.insert(f.root, after, None);
        assert_eq!(f.text(), "a|");

        AnyView::new(7u32).rebuild(&mut state, &mut f.cx());
        assert_eq!(f.text(), "7|");

        state.unmount(&f.dom);
        assert_eq!(f.text(), "|");
        f.window.unmount();
    }
}
