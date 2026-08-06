//! Waiting for something.

use core::future::Future;

use zgui_reactive::prelude::*;
use zgui_reactive::{
    LocalStorage, Owner, RwSignal, provide_local_context, spawn_local, use_local_context,
};

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::{Anchor, AnyView, AnyViewState, ReactiveState, View};

/// How many asynchronous values a subtree is still waiting for.
///
/// Provided by [`Suspense`] and [`Transition`], and taken a share in by every [`Await`] below
/// them. A component that resolves something asynchronously by other means takes a share the same
/// way, with [`SuspenseContext::pending`].
#[derive(Clone, Copy)]
pub struct SuspenseContext {
    /// How many values are outstanding.
    outstanding: RwSignal<usize, LocalStorage>,
    /// Whether everything has been resolved at least once.
    resolved_once: RwSignal<bool, LocalStorage>,
}

impl SuspenseContext {
    /// A context with nothing outstanding.
    pub fn new() -> Self {
        Self {
            outstanding: RwSignal::new_local(0),
            resolved_once: RwSignal::new_local(false),
        }
    }

    /// The context of the nearest enclosing boundary, when there is one.
    pub fn nearest() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Whether anything is still outstanding.
    pub fn pending(&self) -> bool {
        self.outstanding.try_get().unwrap_or_default() > 0
    }

    /// Whether everything has been resolved at least once.
    pub fn has_resolved(&self) -> bool {
        self.resolved_once.try_get().unwrap_or_default()
    }

    /// Records that one more value is outstanding.
    pub fn expect_one(&self) {
        self.outstanding.try_update(|count| *count += 1);
    }

    /// Records that one outstanding value has arrived.
    ///
    /// Called from wherever the value arrived — a finished future, a completed request — which is
    /// not a place that is watching anything, so the count is read here without subscribing to it.
    pub fn resolve_one(&self) {
        self.outstanding
            .try_update(|count| *count = count.saturating_sub(1));
        let still_waiting = self.outstanding.try_get_untracked().unwrap_or_default() > 0;
        if !still_waiting {
            self.resolved_once.try_set(true);
        }
    }
}

impl Default for SuspenseContext {
    fn default() -> Self {
        Self::new()
    }
}

/// What a suspense boundary retains.
pub struct SuspenseState {
    /// The children, which render nothing where they are still waiting.
    children: AnyViewState,
    /// The fallback, which comes and goes.
    fallback: ReactiveState<AnyViewState>,
    /// The scope the boundary's context and children belong to.
    owner: Owner,
}

impl Anchor for SuspenseState {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        self.children.mount(dom, parent, before);
        self.fallback.mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.fallback.unmount(dom);
        self.children.unmount(dom);
        self.owner.cleanup();
    }

    fn first_node(&self) -> Option<NodeId> {
        self.children
            .first_node()
            .or_else(|| self.fallback.first_node())
    }
}

/// Shows a fallback while the asynchronous values below it are still arriving.
///
/// The children are built immediately and stay built: an [`Await`] that has nothing yet renders
/// nothing, so there is no flash of half-finished content, and the values it is waiting for keep
/// loading rather than being cancelled and started again.
///
/// ```
/// use zgui_reactive::{Mounted, flush, install};
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::{
///     Anchor, AnyView, Await, BuildCxOwned, DocumentId, DomHandle, HostHandle, Suspense, View,
/// };
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
/// let mut state = window.with(|| {
///     Suspense::new(
///         || AnyView::new("loading"),
///         || AnyView::new(Await::new(async { 7u32 }, |value| AnyView::new(value.to_string()))),
///     )
///     .build(&mut cx.cx())
/// });
/// state.mount(&dom, root, None);
/// assert_eq!(backend.text_content(root), "loading");
///
/// flush(); // the future completes and the boundary settles
/// flush();
/// assert_eq!(backend.text_content(root), "7");
/// window.unmount();
/// ```
pub struct Suspense<F, C> {
    /// What to show while something is outstanding.
    fallback: F,
    /// The content.
    children: C,
    /// Whether the fallback is shown only before the first resolution.
    only_first_load: bool,
}

impl<F, C> Suspense<F, C>
where
    F: Fn() -> AnyView + 'static,
    C: FnOnce() -> AnyView + 'static,
{
    /// Shows `fallback` while anything below `children` is outstanding.
    pub fn new(fallback: F, children: C) -> Self {
        Self {
            fallback,
            children,
            only_first_load: false,
        }
    }
}

impl<F, C> View for Suspense<F, C>
where
    F: Fn() -> AnyView + 'static,
    C: FnOnce() -> AnyView + 'static,
{
    type State = SuspenseState;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let scoped = cx.child();
        let context = scoped.owner().with(|| {
            let context = SuspenseContext::new();
            provide_local_context(context);
            context
        });

        // The children are built first, so that everything below them has taken its share in the
        // context before the fallback asks whether anything is outstanding — and they are built
        // *inside* the boundary's own scope, because that is where the context was provided and a
        // component that takes its own share does so while its body runs, not while it is built.
        let children = scoped
            .owner()
            .with(|| (self.children)().build(&mut scoped.cx()));

        let fallback = self.fallback;
        let only_first_load = self.only_first_load;
        let fallback = (move || {
            let waiting = context.pending() && !(only_first_load && context.has_resolved());
            if waiting {
                fallback()
            } else {
                AnyView::new(())
            }
        })
        .build(&mut scoped.cx());

        SuspenseState {
            children,
            fallback,
            owner: scoped.owner().clone(),
        }
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        // Rebuilding a boundary means rebuilding what is inside it, which would restart every
        // asynchronous value below. Replacing it wholesale is both simpler and what an author
        // means by it, so the old one is taken out and a new one goes in its place.
        let replacement = self.build(cx);
        let mut old = core::mem::replace(state, replacement);
        old.unmount(cx.dom());
    }
}

/// Shows a fallback only while the *first* load is outstanding.
///
/// The difference from [`Suspense`] is what happens on the second load: a suspense boundary shows
/// its fallback again, and a transition keeps showing what it already has. That is what makes a
/// filter or a page change feel like a change rather than like a reload.
pub struct Transition<F, C>(Suspense<F, C>);

impl<F, C> Transition<F, C>
where
    F: Fn() -> AnyView + 'static,
    C: FnOnce() -> AnyView + 'static,
{
    /// Shows `fallback` only until `children` have resolved for the first time.
    pub fn new(fallback: F, children: C) -> Self {
        Self(Suspense {
            fallback,
            children,
            only_first_load: true,
        })
    }
}

impl<F, C> View for Transition<F, C>
where
    F: Fn() -> AnyView + 'static,
    C: FnOnce() -> AnyView + 'static,
{
    type State = SuspenseState;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        self.0.build(cx)
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        self.0.rebuild(state, cx);
    }
}

/// Renders an asynchronous value once it arrives, and nothing before that.
///
/// Takes a share in the nearest [`Suspense`] or [`Transition`], which is what makes those two
/// mean anything: without something registering as outstanding, a boundary has nothing to wait
/// for.
pub struct Await<Fut, VF, T> {
    /// What is being waited for.
    future: Fut,
    /// What to render once it arrives.
    children: VF,
    /// The type the two agree on.
    value: core::marker::PhantomData<fn() -> T>,
}

impl<Fut, VF, T> Await<Fut, VF, T>
where
    Fut: Future<Output = T> + 'static,
    VF: Fn(T) -> AnyView + 'static,
    T: Clone + 'static,
{
    /// Renders `children` once `future` produces a value.
    pub fn new(future: Fut, children: VF) -> Self {
        Self {
            future,
            children,
            value: core::marker::PhantomData,
        }
    }

    /// The closure this is really made of.
    fn source(self) -> impl FnMut() -> AnyView + 'static {
        let Self {
            future, children, ..
        } = self;
        let value: RwSignal<Option<T>, LocalStorage> = RwSignal::new_local(None);
        let context = SuspenseContext::nearest();
        if let Some(context) = context {
            context.expect_one();
        }
        spawn_local(async move {
            let resolved = future.await;
            value.try_set(Some(resolved));
            if let Some(context) = context {
                context.resolve_one();
            }
        });
        move || match value.try_get().flatten() {
            Some(resolved) => children(resolved),
            None => AnyView::new(()),
        }
    }
}

impl<Fut, VF, T> View for Await<Fut, VF, T>
where
    Fut: Future<Output = T> + 'static,
    VF: Fn(T) -> AnyView + 'static,
    T: Clone + 'static,
{
    type State = ReactiveState<AnyViewState>;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        cx.with_owner(|| self.source()).build(cx)
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        let source = cx.with_owner(|| self.source());
        source.rebuild(state, cx);
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use std::rc::Rc;

    use zgui_reactive::flush;

    use super::{Await, Suspense, SuspenseContext, Transition};
    use crate::fixture::Fixture;
    use crate::view::{Anchor, AnyView, View};

    #[test]
    fn the_fallback_shows_until_the_value_arrives_and_then_the_content_does() {
        let f = Fixture::new();
        let mut state = f.window.with(|| {
            Suspense::new(
                || AnyView::new("loading"),
                || {
                    AnyView::new(Await::new(async { 7u32 }, |value| {
                        AnyView::new(value.to_string())
                    }))
                },
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        assert_eq!(f.text(), "loading");

        flush();
        flush();
        assert_eq!(f.text(), "7");
        f.window.unmount();
    }

    #[test]
    fn a_transition_shows_its_fallback_only_before_the_first_value_arrives() {
        let f = Fixture::new();
        let mut state = f.window.with(|| {
            Transition::new(
                || AnyView::new("loading"),
                || {
                    AnyView::new(Await::new(async { 1u32 }, |value| {
                        AnyView::new(value.to_string())
                    }))
                },
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        assert_eq!(f.text(), "loading");

        flush();
        flush();
        assert_eq!(f.text(), "1");
        f.window.unmount();
    }

    /// One boundary, and a handle on the share its children took, so a test can resolve and
    /// re-request by hand — which is the only way to reach a *second* load.
    fn boundary(
        f: &Fixture,
        transition: bool,
    ) -> (super::SuspenseState, Rc<Cell<Option<SuspenseContext>>>) {
        let taken: Rc<Cell<Option<SuspenseContext>>> = Rc::new(Cell::new(None));
        let record = Rc::clone(&taken);
        let children = move || {
            let context = SuspenseContext::nearest().expect("a boundary is above these children");
            context.expect_one();
            record.set(Some(context));
            AnyView::new("content")
        };
        let state = f.window.with(|| {
            if transition {
                Transition::new(|| AnyView::new("loading"), children).build(&mut f.cx())
            } else {
                Suspense::new(|| AnyView::new("loading"), children).build(&mut f.cx())
            }
        });
        (state, taken)
    }

    #[test]
    fn a_component_takes_its_share_of_the_boundary_while_its_own_body_runs() {
        // A component that resolves something by means other than `Await` takes its share from its
        // body, which runs when the children closure is called — so that call has to happen inside
        // the boundary's own scope, where the context was provided. It did not, and the share was
        // taken from a context that was not there.
        let f = Fixture::new();
        let (mut state, taken) = boundary(&f, false);
        state.mount(&f.dom, f.root, None);
        assert!(taken.get().is_some(), "the children saw the boundary");
        assert_eq!(f.text(), "contentloading");
        f.window.unmount();
    }

    #[test]
    fn a_second_load_shows_a_suspense_fallback_again_and_leaves_a_transition_showing() {
        // The one thing that distinguishes the two. Without a second load in it, a test of
        // `Transition` is a test of `Suspense` under another name.
        let f = Fixture::new();

        let (mut suspended, suspense_share) = boundary(&f, false);
        suspended.mount(&f.dom, f.root, None);
        let context = suspense_share.get().expect("the share was taken");
        context.resolve_one();
        flush();
        assert_eq!(f.text(), "content");
        context.expect_one();
        flush();
        assert_eq!(f.text(), "contentloading", "a suspense waits again");
        suspended.unmount(&f.dom);

        let (mut transitioned, transition_share) = boundary(&f, true);
        transitioned.mount(&f.dom, f.root, None);
        let context = transition_share.get().expect("the share was taken");
        context.resolve_one();
        flush();
        assert_eq!(f.text(), "content");
        context.expect_one();
        flush();
        assert_eq!(
            f.text(),
            "content",
            "a transition keeps what it already has"
        );
        transitioned.unmount(&f.dom);
        f.window.unmount();
    }

    #[test]
    fn an_await_outside_a_boundary_still_renders_its_value() {
        let f = Fixture::new();
        let mut state = f
            .window
            .with(|| Await::new(async { "done" }, AnyView::new).build(&mut f.cx()));
        state.mount(&f.dom, f.root, None);
        assert_eq!(f.text(), "");

        flush();
        flush();
        assert_eq!(f.text(), "done");
        f.window.unmount();
    }

    #[test]
    fn unmounting_a_boundary_stops_the_await_it_was_waiting_on() {
        let f = Fixture::new();
        let dropped = Rc::new(Cell::new(false));

        /// Records, when it is dropped, that the future holding it went away.
        struct Witness(Rc<Cell<bool>>);
        impl Drop for Witness {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let witness = Witness(Rc::clone(&dropped));
        let mut state = f.window.with(|| {
            Await::new(
                async move {
                    let _held = witness;
                    core::future::pending::<&'static str>().await
                },
                AnyView::new,
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        flush();
        assert!(!dropped.get(), "it is still waiting");

        // An `Await` spawns a task, and that task belongs to the scope that built it. Without
        // that, a boundary the user navigated away from would keep its request alive and then
        // write into a signal whose arena entry had already gone.
        state.unmount(&f.dom);
        f.window.unmount();
        assert!(dropped.get(), "the unmount cancelled it");
    }
}
