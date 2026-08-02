//! Showing one thing or another.

mod props;

pub use crate::flow::show::props::{ShowProps, ShowPropsBuilder};

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::flow::branch::Branch;
use crate::id::NodeId;
use crate::view::{Anchor, AnyView, ChildrenFn, View};

/// What a conditional retains: the branch that is showing, and the effect watching the condition.
pub struct ShowState(Branch<bool>);

impl Anchor for ShowState {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        self.0.mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.0.unmount(dom);
    }

    fn first_node(&self) -> Option<NodeId> {
        self.0.first_node()
    }
}

/// Shows its children while a condition holds, and its fallback otherwise.
///
/// The condition is read inside one effect, and it is the *answer* that is compared, not the
/// signals the condition was computed from: a test written `move || items.get().is_empty()` swaps
/// the branch when the list becomes empty and does nothing at all when a row is added to a list
/// that was already full. Flipping it swaps exactly one subtree and touches nothing else.
///
/// Both branches are built afresh when they are shown: a branch that is not showing holds no
/// nodes, no signals and no timers.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, RwSignal, flush, install};
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::{Anchor, AnyView, BuildCxOwned, DocumentId, DomHandle, HostHandle, Show, View};
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
/// let open = window.with(|| RwSignal::new(false));
/// let mut state = window.with(|| {
///     Show::new(move || open.get(), || AnyView::new("open"))
///         .fallback(|| AnyView::new("closed"))
///         .build(&mut cx.cx())
/// });
/// state.mount(&dom, root, None);
/// assert_eq!(backend.text_content(root), "closed");
///
/// open.set(true);
/// flush();
/// assert_eq!(backend.text_content(root), "open");
/// window.unmount();
/// ```
pub struct Show<W> {
    /// The condition.
    when: W,
    /// What to show while it holds.
    children: ChildrenFn,
    /// What to show while it does not.
    fallback: ChildrenFn,
}

impl<W: Fn() -> bool + 'static> Show<W> {
    /// Shows `children` while `when` holds, and nothing otherwise.
    pub fn new(when: W, children: impl Fn() -> AnyView + 'static) -> Self {
        Self {
            when,
            children: ChildrenFn::new(children),
            fallback: ChildrenFn::new(|| AnyView::new(())),
        }
    }

    /// Shows `fallback` while the condition does not hold.
    #[must_use]
    pub fn fallback(mut self, fallback: impl Fn() -> AnyView + 'static) -> Self {
        self.fallback = ChildrenFn::new(fallback);
        self
    }

    /// The two closures this is really made of: the answer, and the branch for an answer.
    fn parts(
        self,
    ) -> (
        impl FnMut() -> bool + 'static,
        impl Fn(&bool) -> AnyView + 'static,
    ) {
        let Self {
            when,
            children,
            fallback,
        } = self;
        let branch = move |showing: &bool| {
            if *showing {
                children.view()
            } else {
                fallback.view()
            }
        };
        (move || when(), branch)
    }
}

impl<W: Fn() -> bool + 'static> View for Show<W> {
    type State = ShowState;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let (when, branch) = self.parts();
        ShowState(Branch::new(when, branch, cx))
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        let (when, branch) = self.parts();
        state.0.restart(when, branch, cx);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use zgui_reactive::prelude::*;
    use zgui_reactive::{RwSignal, flush, on_cleanup_local};

    use super::Show;
    use crate::fixture::Fixture;
    use crate::view::{Anchor, AnyView, View};

    #[test]
    fn the_branch_that_is_not_showing_holds_nothing() {
        let f = Fixture::new();
        let open = f.window.with(|| RwSignal::new(true));
        let cleaned = Rc::new(Cell::new(false));
        let flag = Rc::clone(&cleaned);

        let mut state = f.window.with(|| {
            Show::new(
                move || open.get(),
                move || {
                    let flag = Rc::clone(&flag);
                    AnyView::new(move || {
                        let flag = Rc::clone(&flag);
                        on_cleanup_local(move || flag.set(true));
                        "inside"
                    })
                },
            )
            .fallback(|| AnyView::new("away"))
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        assert_eq!(f.text(), "inside");

        open.set(false);
        flush();
        assert_eq!(f.text(), "away");
        assert!(cleaned.get(), "the branch that went away was cleaned up");
        f.window.unmount();
    }

    #[test]
    fn a_write_that_does_not_change_the_answer_leaves_the_branch_untouched() {
        // The branch counts its own builds, because "the nodes look the same" is not the property:
        // rebuilding a branch runs its cleanups, cancels its timers and rebinds its handles, and a
        // branch made of a static string would hide all three.
        let f = Fixture::new();
        let source = f.window.with(|| RwSignal::new(2));
        let builds = Rc::new(Cell::new(0));
        let cleanups = Rc::new(Cell::new(0));
        let counter = Rc::clone(&builds);
        let cleaned = Rc::clone(&cleanups);

        let mut state = f.window.with(|| {
            Show::new(
                move || source.get() % 2 == 0,
                move || {
                    let counter = Rc::clone(&counter);
                    let cleaned = Rc::clone(&cleaned);
                    AnyView::new(move || {
                        counter.set(counter.get() + 1);
                        let cleaned = Rc::clone(&cleaned);
                        on_cleanup_local(move || cleaned.set(cleaned.get() + 1));
                        "even"
                    })
                },
            )
            .fallback(|| AnyView::new("odd"))
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        let node = state.first_node();
        assert_eq!(builds.get(), 1);

        // Two more writes to what the condition reads, neither of which changes the answer.
        source.set(4);
        flush();
        source.set(6);
        flush();
        assert_eq!(f.text(), "even");
        assert_eq!(state.first_node(), node, "nothing was replaced");
        assert_eq!(builds.get(), 1, "the branch was not built again");
        assert_eq!(cleanups.get(), 0, "and nothing under it was disposed of");

        // ... and the answer changing still swaps.
        source.set(7);
        flush();
        assert_eq!(f.text(), "odd");
        assert_eq!(cleanups.get(), 1);
        f.window.unmount();
    }

    #[test]
    fn a_rebuild_whose_answer_is_unchanged_keeps_the_branch_that_is_showing() {
        let f = Fixture::new();
        let open = f.window.with(|| RwSignal::new(true));
        let mut state = f.window.with(|| {
            Show::new(move || open.get(), || AnyView::new("open"))
                .fallback(|| AnyView::new("shut"))
                .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        let node = state.first_node();

        // What an enclosing hole re-running does: a fresh `Show` over the same state.
        f.window.with(|| {
            Show::new(move || open.get(), || AnyView::new("open"))
                .fallback(|| AnyView::new("shut"))
                .rebuild(&mut state, &mut f.cx());
        });
        assert_eq!(f.text(), "open");
        assert_eq!(state.first_node(), node, "the branch kept its nodes");

        // The replacement effect is the one now watching the condition.
        open.set(false);
        flush();
        assert_eq!(f.text(), "shut");
        f.window.unmount();
    }
}
