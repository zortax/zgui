//! A view chosen at run time.

use crate::cx::BuildCx;
use crate::view::{AnyView, AnyViewState, ReactiveState, View};

/// Renders whichever view its closure returns, and swaps when the answer changes.
///
/// Where [`Show`](crate::Show) chooses between two branches written in advance, this chooses among
/// however many the closure can produce: a router, a stepper, a panel whose contents are decided
/// by a signal.
///
/// A run that returns a view of the *same* type as the last one rebuilds in place. Only a change
/// of view type replaces nodes.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, RwSignal, flush, install};
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::{Anchor, AnyView, BuildCxOwned, DocumentId, DomHandle, Dynamic, HostHandle, View};
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
/// let page = window.with(|| RwSignal::new(0));
/// let mut state = window.with(|| {
///     Dynamic::new(move || match page.get() {
///         0 => AnyView::new("first"),
///         _ => AnyView::new(42u32),
///     })
///     .build(&mut cx.cx())
/// });
/// state.mount(&dom, root, None);
/// assert_eq!(backend.text_content(root), "first");
///
/// page.set(1);
/// flush();
/// assert_eq!(backend.text_content(root), "42");
/// window.unmount();
/// ```
pub struct Dynamic<F>(F);

impl<F: FnMut() -> AnyView + 'static> Dynamic<F> {
    /// Renders whatever `view` returns.
    pub fn new(view: F) -> Self {
        Self(view)
    }
}

impl<F: FnMut() -> AnyView + 'static> View for Dynamic<F> {
    type State = ReactiveState<AnyViewState>;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        self.0.build(cx)
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        self.0.rebuild(state, cx);
    }
}

#[cfg(test)]
mod tests {
    use zgui_reactive::prelude::*;
    use zgui_reactive::{RwSignal, flush};

    use super::Dynamic;
    use crate::fixture::Fixture;
    use crate::view::{Anchor, AnyView, View};

    #[test]
    fn the_same_view_type_is_rebuilt_and_a_different_one_replaces() {
        let f = Fixture::new();
        let page = f.window.with(|| RwSignal::new(0));
        let mut state = f.window.with(|| {
            Dynamic::new(move || match page.get() {
                0 => AnyView::new("zero"),
                1 => AnyView::new("one"),
                _ => AnyView::new(9u32),
            })
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        let node = state.first_node();

        page.set(1);
        flush();
        assert_eq!(f.text(), "one");
        assert_eq!(
            state.first_node(),
            node,
            "the same view type reused its node"
        );

        page.set(2);
        flush();
        assert_eq!(f.text(), "9");
        f.window.unmount();
    }
}
