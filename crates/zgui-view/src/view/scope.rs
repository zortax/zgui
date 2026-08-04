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
pub struct Scoped<F> {
    /// The body.
    body: F,
    /// Which component this is an instance of, when it is one.
    ///
    /// Always carried, whether or not anything reads it: it is one `&'static` word, it lets
    /// `#[component]` expand to the same code in every build, and a macro that had to know which
    /// features its caller enabled would be a macro that breaks when a dependency turns one on.
    #[cfg_attr(
        not(feature = "instrument"),
        expect(
            dead_code,
            reason = "carried in every build so the macro expands to one thing; read only when \
                      the instrument feature is on"
        )
    )]
    meta: Option<&'static ComponentMeta>,
}

/// Where a component was declared, and what it is called.
///
/// Written by `#[component]` as a constant of the props type, so the strings are in the binary
/// rather than built at run time and the whole record costs nothing to carry around.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentMeta {
    /// The component's path, as `module::path::Name`.
    pub name: &'static str,
    /// The file it was declared in.
    pub file: &'static str,
    /// The line it was declared on.
    pub line: u32,
}

impl<F, V> Scoped<F>
where
    F: FnOnce() -> V + 'static,
    V: IntoView,
{
    /// Wraps a body in a scope of its own.
    pub fn new(body: F) -> Self {
        Self { body, meta: None }
    }

    /// The same, for a body that is one instance of the component `meta` describes.
    ///
    /// What `#[component]` calls. With the `instrument` feature on, the content is bracketed by a
    /// pair of marker nodes a development tool can read the component's extent from; without it,
    /// this is [`Scoped::new`] carrying one extra word it never looks at.
    pub fn named(meta: &'static ComponentMeta, body: F) -> Self {
        Self {
            body,
            meta: Some(meta),
        }
    }
}

/// What a scoped view retains: the scope, and the state of what the body built.
pub struct ScopedState<S> {
    /// The scope. Cleaning it up frees everything the body allocated.
    owner: Owner,
    /// The built body.
    inner: S,
    /// The markers bracketing the content, when this is an instrumented component instance.
    #[cfg(feature = "instrument")]
    markers: Option<(NodeId, NodeId)>,
}

impl<F, V> View for Scoped<F>
where
    F: FnOnce() -> V + 'static,
    V: IntoView,
{
    type State = ScopedState<<V::Output as View>::State>;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let owner = cx.owner().child();
        // The pair, made before the body runs so the registration is in place by the time anything
        // inside it is built — a tool sampling mid-build would otherwise see the content of a
        // component whose boundary it has no name for.
        #[cfg(feature = "instrument")]
        let markers = self.meta.map(|meta| {
            let pair = (cx.dom().create_marker(), cx.dom().create_marker());
            crate::instrument::register(pair.0, pair.1, meta, &owner);
            pair
        });
        let scoped = cx.to_owned_cx().with_owner(owner.clone());
        let inner = owner.with(|| (self.body)().into_view().build(&mut scoped.cx()));
        ScopedState {
            owner,
            inner,
            #[cfg(feature = "instrument")]
            markers,
        }
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        let scoped = cx.to_owned_cx().with_owner(state.owner.clone());
        // The previous run's signals, memos and cleanups go before this one's are made, because
        // the scope outlives every run of the body and would otherwise accumulate one run's worth
        // of reactive state per rebuild for as long as the view stays mounted.
        state.owner.with_cleanup(|| {
            (self.body)()
                .into_view()
                .rebuild(&mut state.inner, &mut scoped.cx());
        });
    }
}

/// What the registry holds is forgotten when the state that made it goes, whether or not anybody
/// unmounted it first.
///
/// Tied to the drop rather than to the unmount because those are not the same event: content
/// replaced inside a hole is unmounted and dropped, but a state dropped without being unmounted —
/// which is what happens when a whole subtree is discarded — would otherwise leave its pair in the
/// map for the life of the program. The map is keyed on nodes that no longer exist at that point,
/// so the leak is also a source of wrong answers, not only of memory.
#[cfg(feature = "instrument")]
impl<S> Drop for ScopedState<S> {
    fn drop(&mut self) {
        if let Some((open, close)) = self.markers {
            crate::instrument::deregister(open, close);
        }
    }
}

impl<S: Anchor> Anchor for ScopedState<S> {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        // Both markers first, then the content between them: what the pair means is "everything
        // from here to there came from this component", so the content has to land inside it.
        #[cfg(feature = "instrument")]
        if let Some((open, close)) = self.markers {
            dom.insert(parent, open, before);
            dom.insert(parent, close, before);
            self.inner.mount(dom, parent, Some(close));
            return;
        }
        self.inner.mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.inner.unmount(dom);
        #[cfg(feature = "instrument")]
        if let Some((open, close)) = self.markers {
            dom.detach(open);
            dom.detach(close);
        }
        self.owner.cleanup();
    }

    fn first_node(&self) -> Option<NodeId> {
        // The open marker when there is one: it is placed before anything the body built and stays
        // put across a rebuild, which makes it a steadier answer than the body's own first node —
        // that moves whenever the first thing the body renders is replaced.
        #[cfg(feature = "instrument")]
        if let Some((open, _)) = self.markers {
            return Some(open);
        }
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

    /// A named scope brackets what it built, and stops claiming those nodes when it goes.
    ///
    /// The whole contract the component tree is read through: an open marker, the content, a close
    /// marker, and nothing left in the registry once the state is dropped.
    #[cfg(feature = "instrument")]
    #[test]
    fn a_named_scope_brackets_its_content_and_forgets_it_afterwards() {
        use crate::instrument::{MarkerRole, at};
        use crate::view::scope::ComponentMeta;

        static META: ComponentMeta = ComponentMeta {
            name: "demo::Widget",
            file: "demo.rs",
            line: 7,
        };

        let fixture = Fixture::new();
        let mut state = Scoped::named(&META, || "body").build(&mut fixture.cx());
        state.mount(&fixture.dom, fixture.root, None);

        let open = state.first_node().expect("the open marker is the first node");
        let Some(MarkerRole::Open(tag)) = at(open) else {
            panic!("the scope's first node is not a registered component boundary");
        };
        assert_eq!(tag.name, "demo::Widget");
        assert_eq!((tag.file, tag.line), ("demo.rs", 7));
        // The content is still the content: a boundary that changed what the document says would
        // be a boundary that changed the program.
        assert_eq!(fixture.text(), "body");

        state.unmount(&fixture.dom);
        drop(state);
        assert_eq!(
            at(open),
            None,
            "the pair outlived the state that registered it"
        );
    }

    /// An unnamed scope is exactly what it was before: no markers, no registration.
    ///
    /// What keeps the feature honest. Every conditional, list and hole in the program is an
    /// unnamed scope, and instrumenting those would put a boundary in the tree per control flow
    /// construct rather than per component.
    #[cfg(feature = "instrument")]
    #[test]
    fn an_unnamed_scope_registers_nothing() {
        use crate::instrument::at;

        let fixture = Fixture::new();
        let mut state = Scoped::new(|| "body").build(&mut fixture.cx());
        state.mount(&fixture.dom, fixture.root, None);

        if let Some(first) = state.first_node() {
            assert_eq!(at(first), None, "an unnamed scope registered a boundary");
        }
        assert_eq!(fixture.text(), "body");
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
