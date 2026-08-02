//! Errors a view produced, and the boundary that catches them.

use core::fmt::{self, Debug, Display};
use std::rc::Rc;

use zgui_reactive::prelude::*;
use zgui_reactive::{LocalStorage, Owner, RwSignal, provide_local_context, use_local_context};

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::{Anchor, AnyView, AnyViewState, Hole, ReactiveState, View};

/// Something a view could not do.
///
/// Deliberately thin: it carries a message and, when there was one, the error it came from. A
/// boundary shows it to the user or logs it; nothing in the view layer inspects it.
#[derive(Clone)]
pub struct ViewError(Rc<str>);

impl ViewError {
    /// An error described by `message`.
    pub fn new(message: impl Display) -> Self {
        Self(Rc::from(message.to_string().as_str()))
    }

    /// The message.
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl Display for ViewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Debug for ViewError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("ViewError").field(&&*self.0).finish()
    }
}

/// Where a failing view reports what went wrong.
#[derive(Clone)]
struct ErrorSink(RwSignal<Vec<ViewError>, LocalStorage>);

/// Reports `error` to the nearest enclosing boundary.
///
/// With no boundary above, the error is dropped: a view layer that panicked here would turn a
/// recoverable failure into a dead window.
pub fn report_error(error: ViewError) {
    if let Some(sink) = use_local_context::<ErrorSink>() {
        sink.0.try_update(|errors| errors.push(error));
    }
}

/// What a fallible view retains.
pub struct ResultState<S>(Hole<S>);

impl<S: crate::view::Anchor> crate::view::Anchor for ResultState<S> {
    fn mount(
        &mut self,
        dom: &crate::dom::DomHandle,
        parent: crate::id::NodeId,
        before: Option<crate::id::NodeId>,
    ) {
        self.0.mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &crate::dom::DomHandle) {
        self.0.unmount(dom);
    }

    fn first_node(&self) -> Option<crate::id::NodeId> {
        self.0.first_node()
    }
}

impl<V: View, E: Display + 'static> View for Result<V, E> {
    type State = ResultState<V::State>;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let mut hole = Hole::new(cx.dom());
        match self {
            Ok(view) => {
                let built = view.build(cx);
                hole.fill(cx.dom(), built);
            }
            Err(error) => report_error(ViewError::new(error)),
        }
        ResultState(hole)
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        match (self, state.0.content_mut()) {
            (Ok(view), Some(existing)) => view.rebuild(existing, cx),
            (Ok(view), None) => {
                let built = view.build(cx);
                state.0.set(cx.dom(), Some(built));
            }
            (Err(error), _) => {
                state.0.set(cx.dom(), None);
                report_error(ViewError::new(error));
            }
        }
    }
}

/// Shows its children, or its fallback once one of them has reported an error.
///
/// Errors reach it from any depth: a `Result` anywhere below reports to the nearest boundary above
/// it, so a failure in one panel does not take the window with it.
///
/// The swap happens at the next flush rather than during the build that failed, because a view
/// reports its error *while it is being built* and the boundary is already building at that
/// moment. Nothing of the failed branch is shown in the meantime — a failing `Result` renders
/// nothing at all — so what is visible for that one frame is the boundary's other children.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, RwSignal, flush, install};
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::{
///     Anchor, AnyView, BuildCxOwned, DocumentId, DomHandle, ErrorBoundary, HostHandle, View,
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
///     ErrorBoundary::new(
///         || AnyView::new(Err::<&'static str, _>("the file was not there")),
///         |errors| AnyView::new(errors[0].message().to_owned()),
///     )
///     .build(&mut cx.cx())
/// });
/// state.mount(&dom, root, None);
///
/// flush();
/// assert_eq!(backend.text_content(root), "the file was not there");
/// window.unmount();
/// ```
pub struct ErrorBoundary<C, F> {
    /// The content.
    children: C,
    /// What to show once it has failed.
    fallback: F,
}

impl<C, F> ErrorBoundary<C, F>
where
    C: Fn() -> AnyView + 'static,
    F: Fn(&[ViewError]) -> AnyView + 'static,
{
    /// Shows `children`, or `fallback` once one of them reports an error.
    pub fn new(children: C, fallback: F) -> Self {
        Self { children, fallback }
    }

    /// The closure this is really made of, reporting into `errors`.
    fn source(
        self,
        errors: RwSignal<Vec<ViewError>, LocalStorage>,
    ) -> impl FnMut() -> AnyView + 'static {
        let Self { children, fallback } = self;
        move || {
            let reported = errors.try_get().unwrap_or_default();
            if reported.is_empty() {
                children()
            } else {
                fallback(&reported)
            }
        }
    }
}

/// What a boundary retains.
pub struct ErrorBoundaryState {
    /// The children, or the fallback once one of them has failed.
    content: ReactiveState<AnyViewState>,
    /// What has been reported so far.
    errors: RwSignal<Vec<ViewError>, LocalStorage>,
    /// The scope holding the sink every view below reports into.
    owner: Owner,
}

impl Anchor for ErrorBoundaryState {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        self.content.mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.content.unmount(dom);
        self.owner.cleanup();
    }

    fn first_node(&self) -> Option<NodeId> {
        self.content.first_node()
    }
}

impl<C, F> View for ErrorBoundary<C, F>
where
    C: Fn() -> AnyView + 'static,
    F: Fn(&[ViewError]) -> AnyView + 'static,
{
    type State = ErrorBoundaryState;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        // The sink is provided on a scope of the boundary's own, which every view built below it
        // descends from. Providing it from inside the effect instead would put it on the effect's
        // scope, which is a *sibling* of everything the effect builds — so a failure reported one
        // conditional deeper would find no boundary and be dropped.
        let scoped = cx.child();
        let errors = scoped.owner().with(|| {
            let errors: RwSignal<Vec<ViewError>, LocalStorage> = RwSignal::new_local(Vec::new());
            provide_local_context(ErrorSink(errors));
            errors
        });
        let content = scoped
            .owner()
            .with(|| self.source(errors).build(&mut scoped.cx()));
        ErrorBoundaryState {
            content,
            errors,
            owner: scoped.owner().clone(),
        }
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        let scoped = cx.to_owned_cx().with_owner(state.owner.clone());
        let errors = state.errors;
        scoped.owner().with(|| {
            self.source(errors)
                .rebuild(&mut state.content, &mut scoped.cx())
        });
    }
}

#[cfg(test)]
mod tests {
    use zgui_reactive::flush;

    use super::{ErrorBoundary, ViewError};
    use crate::fixture::Fixture;
    use crate::flow::show::Show;
    use crate::view::{Anchor, AnyView, View};

    #[test]
    fn a_failing_child_shows_the_fallback_and_names_the_error() {
        let f = Fixture::new();
        let mut state = f.window.with(|| {
            ErrorBoundary::new(
                || AnyView::new(Err::<&'static str, _>("nope")),
                |errors: &[ViewError]| AnyView::new(errors[0].message().to_owned()),
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);

        flush();
        assert_eq!(f.text(), "nope");
        f.window.unmount();
    }

    #[test]
    fn a_child_that_succeeds_is_shown_and_the_fallback_never_is() {
        let f = Fixture::new();
        let mut state = f.window.with(|| {
            ErrorBoundary::new(
                || AnyView::new(Ok::<&'static str, &'static str>("fine")),
                |_: &[ViewError]| AnyView::new("broken"),
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        flush();
        assert_eq!(f.text(), "fine");
        f.window.unmount();
    }

    #[test]
    fn an_error_reported_from_a_nested_conditional_still_reaches_the_boundary() {
        // "From any depth" is the boundary's whole promise, and a failure reported directly by the
        // boundary's own child exercises none of it: the sink has to hang off a scope that
        // everything built below descends from, not off the effect that built it.
        let f = Fixture::new();
        let mut state = f.window.with(|| {
            ErrorBoundary::new(
                || {
                    AnyView::new(Show::new(
                        || true,
                        || AnyView::new(Err::<&'static str, _>("deep")),
                    ))
                },
                |errors: &[ViewError]| AnyView::new(errors[0].message().to_owned()),
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);

        flush();
        flush();
        assert_eq!(f.text(), "deep");
        f.window.unmount();
    }

    #[test]
    fn an_error_reported_with_no_boundary_above_is_dropped_rather_than_fatal() {
        let f = Fixture::new();
        let mut state = f.window.with(|| {
            AnyView::new(Err::<&'static str, _>("nobody is listening")).build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        flush();
        assert_eq!(f.text(), "");
        f.window.unmount();
    }
}
