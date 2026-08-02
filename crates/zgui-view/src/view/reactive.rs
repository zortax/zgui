//! A closure is a reactive hole.

use core::cell::RefCell;
use std::rc::Rc;

use zgui_reactive::{Owner, RenderEffect};

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::anchor::Anchor;
use crate::view::hole::Hole;
use crate::view::view::{IntoView, View};

/// What a reactive hole retains: a scope, an effect, and the place its content sits.
///
/// The effect is a *render* effect, whose first run happens synchronously inside its constructor.
/// That is what lets `build` hand a real, mounted state back to its parent instead of a hole to be
/// filled on the next poll of the executor.
///
/// [`Anchor::unmount`] disposes of the scope **synchronously**. Leaving that to the effect's own
/// drop defers every cleanup under this hole by one poll, which is one frame in which an unmounted
/// view's timers still fire.
pub struct ReactiveState<S> {
    /// Where the content sits, shared with the effect that replaces it.
    hole: Rc<RefCell<Hole<S>>>,
    /// The scope everything under this hole belongs to.
    owner: Owner,
    /// The effect. Dropping it stops the hole updating.
    effect: Option<RenderEffect<()>>,
}

impl<S: Anchor> ReactiveState<S> {
    /// Builds a hole and the effect that keeps it filled.
    fn start<V>(mut source: impl FnMut() -> V + 'static, cx: &mut BuildCx<'_>) -> Self
    where
        V: IntoView,
        V::Output: View<State = S>,
    {
        let owner = cx.owner().child();
        let hole = Rc::new(RefCell::new(Hole::new(cx.dom())));
        let scoped = cx.to_owned_cx().with_owner(owner.clone());
        let effect = owner.with(|| {
            let hole = Rc::clone(&hole);
            RenderEffect::new(move |previous: Option<()>| {
                let view = source().into_view();
                let mut cx = scoped.cx();
                let mut hole = hole.borrow_mut();
                match (previous, hole.content_mut()) {
                    (Some(()), Some(existing)) => view.rebuild(existing, &mut cx),
                    _ => {
                        let built = view.build(&mut cx);
                        hole.set(scoped.dom(), Some(built));
                    }
                }
            })
        });
        Self {
            hole,
            owner,
            effect: Some(effect),
        }
    }

    /// Replaces the closure behind this hole, keeping the hole and its content where they are.
    ///
    /// A rebuilt closure captures values its predecessor did not, so the old effect is cancelled
    /// and a new one takes over. The new effect's first run rebuilds the content in place, so no
    /// node moves and nothing flickers.
    fn restart<V>(&mut self, mut source: impl FnMut() -> V + 'static, cx: &mut BuildCx<'_>)
    where
        V: IntoView,
        V::Output: View<State = S>,
    {
        self.effect = None;
        let scoped = cx.to_owned_cx().with_owner(self.owner.clone());
        let hole = Rc::clone(&self.hole);
        self.effect = Some(self.owner.with(|| {
            RenderEffect::new(move |_previous: Option<()>| {
                let view = source().into_view();
                let mut cx = scoped.cx();
                let mut hole = hole.borrow_mut();
                match hole.content_mut() {
                    Some(existing) => view.rebuild(existing, &mut cx),
                    None => {
                        let built = view.build(&mut cx);
                        hole.set(scoped.dom(), Some(built));
                    }
                }
            })
        }));
    }
}

impl<S: Anchor> Anchor for ReactiveState<S> {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        self.hole.borrow_mut().mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.effect = None;
        self.hole.borrow_mut().unmount(dom);
        self.owner.cleanup();
    }

    fn first_node(&self) -> Option<NodeId> {
        self.hole.borrow().first_node()
    }
}

impl<F, V> View for F
where
    F: FnMut() -> V + 'static,
    V: IntoView,
{
    type State = ReactiveState<<V::Output as View>::State>;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        ReactiveState::start(self, cx)
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        state.restart(self, cx);
    }
}

#[cfg(test)]
mod tests {
    use zgui_reactive::prelude::*;
    use zgui_reactive::{RwSignal, flush};

    use crate::fixture::Fixture;
    use crate::view::anchor::Anchor;
    use crate::view::view::View;

    #[test]
    fn a_closure_builds_synchronously_and_updates_at_the_flush() {
        let f = Fixture::new();
        let count = f.window.with(|| RwSignal::new(0));

        let mut state = f
            .window
            .with(|| (move || count.get().to_string()).build(&mut f.cx()));
        state.mount(&f.dom, f.root, None);
        assert_eq!(f.text(), "0", "the first run is synchronous");

        count.set(7);
        assert_eq!(f.text(), "0", "and every later run waits for the flush");
        flush();
        assert_eq!(f.text(), "7");

        state.unmount(&f.dom);
        f.window.unmount();
    }

    #[test]
    fn an_unchanged_value_writes_nothing_to_the_backend() {
        let f = Fixture::new();
        let source = f.window.with(|| RwSignal::new(1));
        let mut state = f
            .window
            .with(|| (move || (source.get() % 2).to_string()).build(&mut f.cx()));
        state.mount(&f.dom, f.root, None);
        let node = state.first_node().expect("the hole has a first node");

        source.set(3); // a different signal value, the same rendered text
        flush();
        assert_eq!(f.text(), "1");
        assert_eq!(
            state.first_node(),
            Some(node),
            "the text node was reused rather than replaced"
        );

        state.unmount(&f.dom);
        f.window.unmount();
    }

    #[test]
    fn unmounting_stops_the_effect_before_it_returns() {
        let f = Fixture::new();
        let count = f.window.with(|| RwSignal::new(0));
        let mut state = f
            .window
            .with(|| (move || count.get().to_string()).build(&mut f.cx()));
        state.mount(&f.dom, f.root, None);

        state.unmount(&f.dom);
        count.set(99);
        flush();
        assert_eq!(f.text(), "", "an unmounted hole writes nothing");
        f.window.unmount();
    }

    #[test]
    fn the_scope_under_a_hole_is_disposed_of_synchronously_on_unmount() {
        use std::cell::Cell;
        use std::rc::Rc;
        use zgui_reactive::on_cleanup_local;

        let f = Fixture::new();
        let cleaned = Rc::new(Cell::new(false));
        let flag = Rc::clone(&cleaned);

        let mut state = f.window.with(|| {
            (move || {
                let flag = Rc::clone(&flag);
                on_cleanup_local(move || flag.set(true));
                "content"
            })
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        assert!(!cleaned.get());

        state.unmount(&f.dom);
        assert!(cleaned.get(), "the cleanup ran before unmount returned");
        f.window.unmount();
    }
}
