//! A view with a reactive scope of its own.

use zgui_reactive::Owner;

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::anchor::Anchor;
use crate::view::view::{IntoView, View};

/// A view whose signals, memos and cleanups belong to it rather than to its parent.
///
/// Everything a scoped view allocates is freed the moment it is unmounted, synchronously, however
/// long its parent lives on. That is the difference between a list that frees a row when the row
/// goes away and one that frees it when the whole list does.
///
/// The body runs inside the scope, so anything it creates — including a `provide_context` — is
/// visible to what the body builds and to nothing outside it.
///
/// ```
/// use std::cell::Cell;
/// use std::rc::Rc;
///
/// use zgui_interned::ElementName;
/// use zgui_reactive::{Mounted, install, on_cleanup_local};
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::view::Scoped;
/// use zgui_view::{Anchor, BuildCxOwned, DocumentId, DomHandle, HostHandle, View};
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
/// let cleaned = Rc::new(Cell::new(false));
/// let flag = Rc::clone(&cleaned);
/// let mut state = Scoped::new(move || {
///     on_cleanup_local(move || flag.set(true));
///     "inside"
/// })
/// .build(&mut cx.cx());
/// state.mount(&dom, root, None);
/// assert_eq!(backend.text_content(root), "inside");
/// assert!(!cleaned.get());
///
/// // Unmounting the view runs its scope's cleanups before it returns.
/// state.unmount(&dom);
/// assert!(cleaned.get());
/// window.unmount();
/// ```
pub struct Scoped<F>(F);

impl<F, V> Scoped<F>
where
    F: FnOnce() -> V + 'static,
    V: IntoView,
{
    /// Wraps a body in a scope of its own.
    pub fn new(body: F) -> Self {
        Self(body)
    }
}

/// What a scoped view retains: the scope, and the state of what the body built.
pub struct ScopedState<S> {
    /// The scope. Cleaning it up frees everything the body allocated.
    owner: Owner,
    /// The built body.
    inner: S,
}

impl<F, V> View for Scoped<F>
where
    F: FnOnce() -> V + 'static,
    V: IntoView,
{
    type State = ScopedState<<V::Output as View>::State>;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let owner = cx.owner().child();
        let scoped = cx.to_owned_cx().with_owner(owner.clone());
        let inner = owner.with(|| self.0().into_view().build(&mut scoped.cx()));
        ScopedState { owner, inner }
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        let scoped = cx.to_owned_cx().with_owner(state.owner.clone());
        // The previous run's signals, memos and cleanups go before this one's are made, because
        // the scope outlives every run of the body and would otherwise accumulate one run's worth
        // of reactive state per rebuild for as long as the view stays mounted.
        state.owner.with_cleanup(|| {
            self.0()
                .into_view()
                .rebuild(&mut state.inner, &mut scoped.cx());
        });
    }
}

impl<S: Anchor> Anchor for ScopedState<S> {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        self.inner.mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.inner.unmount(dom);
        self.owner.cleanup();
    }

    fn first_node(&self) -> Option<NodeId> {
        self.inner.first_node()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use zgui_reactive::prelude::*;
    use zgui_reactive::{RwSignal, flush, on_cleanup_local};

    use super::Scoped;
    use crate::fixture::Fixture;
    use crate::view::anchor::Anchor;
    use crate::view::view::View;

    #[test]
    fn a_scope_is_cleaned_up_when_the_view_is_unmounted_and_not_before() {
        let fixture = Fixture::new();
        let cleaned = Rc::new(Cell::new(0u32));
        let counter = Rc::clone(&cleaned);

        let mut state = Scoped::new(move || {
            on_cleanup_local(move || counter.set(counter.get() + 1));
            "body"
        })
        .build(&mut fixture.cx());
        state.mount(&fixture.dom, fixture.root, None);
        assert_eq!(fixture.text(), "body");
        assert_eq!(cleaned.get(), 0);

        state.unmount(&fixture.dom);
        assert_eq!(cleaned.get(), 1);
    }

    /// A scope outlives every run of its body, so a rebuild that did not dispose of the previous
    /// run would keep one run's worth of signals, memos and cleanups alive per rebuild for as
    /// long as the view stayed mounted — unbounded growth driven by an ordinary signal write.
    #[test]
    fn rebuilding_disposes_of_the_previous_run_rather_than_piling_it_up() {
        let fixture = Fixture::new();
        let live = Rc::new(Cell::new(0i32));

        let body = {
            let live = Rc::clone(&live);
            move || {
                let live = Rc::clone(&live);
                live.set(live.get() + 1);
                on_cleanup_local(move || live.set(live.get() - 1));
                "body"
            }
        };

        let mut state = Scoped::new(body.clone()).build(&mut fixture.cx());
        state.mount(&fixture.dom, fixture.root, None);
        assert_eq!(live.get(), 1);

        for _ in 0..16 {
            Scoped::new(body.clone()).rebuild(&mut state, &mut fixture.cx());
            assert_eq!(live.get(), 1, "one run of the body is alive at a time");
        }

        state.unmount(&fixture.dom);
        assert_eq!(live.get(), 0);
    }

    #[test]
    fn a_rebuilt_body_still_drives_what_it_built() {
        let fixture = Fixture::new();
        let outer = fixture.window.with(|| RwSignal::new(0i32));

        // The body makes a signal of its own on every run, and what it returns reads it.
        let body = move || {
            let inner = RwSignal::new(outer.get());
            move || inner.get().to_string()
        };

        let mut state = Scoped::new(body).build(&mut fixture.cx());
        state.mount(&fixture.dom, fixture.root, None);
        assert_eq!(fixture.text(), "0");

        outer.set(5);
        Scoped::new(body).rebuild(&mut state, &mut fixture.cx());
        flush();
        assert_eq!(
            fixture.text(),
            "5",
            "the rebuilt body's own signal drives the rebuilt view"
        );
        state.unmount(&fixture.dom);
    }

    #[test]
    fn a_signal_read_by_the_body_still_drives_what_the_body_built() {
        let fixture = Fixture::new();
        let text = fixture.window.with(|| RwSignal::new("before".to_owned()));

        let mut state = Scoped::new(move || move || text.get()).build(&mut fixture.cx());
        state.mount(&fixture.dom, fixture.root, None);
        assert_eq!(fixture.text(), "before");

        text.set("after".to_owned());
        flush();
        assert_eq!(fixture.text(), "after");
        state.unmount(&fixture.dom);
    }
}
