//! A closure is a reactive hole.

use core::cell::RefCell;
use std::rc::Rc;

use zgui_reactive::{Owner, RenderEffect, Sink};

use crate::cx::{BuildCx, BuildCxOwned};
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::anchor::Anchor;
use crate::view::hole::Hole;
use crate::view::view::{IntoView, View};

/// What a reactive hole retains: a scope, the updater that keeps the content up to date, and the
/// content.
///
/// The first run of the closure happens synchronously in `build`. That is what lets `build` hand
/// a real, mounted state back to its parent instead of a hole to be filled on the next poll of the
/// executor. Later runs happen at the flush.
///
/// Each run of the closure happens in the hole's scope, and the scope is cleaned up before the
/// next run. What one run creates (signals, memos, cleanups) is gone before the next run starts.
///
/// A closure whose views [rebuild in place](View::REBUILDS_IN_PLACE), such as text, is kept up to
/// date by a [`Sink`] and needs no marker node. Any other closure is kept up to date by a
/// [`RenderEffect`], and its content sits before a marker node, so content that replaces it has a
/// place to go.
///
/// A sink-backed hole has the [`Sink`] semantics: in each flush round it runs after the task pool,
/// and [`Owner::pause`] does not stop it. Its cleanups run while the sink tracks reads, so a
/// signal that a cleanup reads also reruns the hole.
///
/// [`Anchor::unmount`] disposes of the scope **synchronously**. Leaving that to the effect's own
/// drop defers every cleanup under this hole by one poll, which is one frame in which an unmounted
/// view's timers still fire.
pub struct ReactiveState<S> {
    /// The content, shared with the updater.
    content: Content<S>,
    /// The scope everything under this hole belongs to.
    owner: Owner,
    /// The sink or effect that keeps the content up to date. Dropping it stops the hole updating.
    updater: Option<Updater>,
}

/// Where the content of a hole sits.
enum Content<S> {
    /// Content that can change shape, before a marker node.
    Hole(Rc<RefCell<Hole<S>>>),
    /// Content that keeps its nodes. Empty only during the first run.
    InPlace(Rc<RefCell<Option<S>>>),
}

/// What keeps a hole's content up to date.
enum Updater {
    /// For content before a marker node.
    #[expect(dead_code, reason = "held for its drop, which stops the effect")]
    Effect(RenderEffect<()>),
    /// For content that keeps its nodes.
    #[expect(dead_code, reason = "held for its drop, which stops the sink")]
    Sink(Sink),
}

impl<S: Anchor> ReactiveState<S> {
    /// Builds a hole and the updater that keeps it filled.
    fn start<V>(source: impl FnMut() -> V + 'static, cx: &mut BuildCx<'_>) -> Self
    where
        V: IntoView,
        V::Output: View<State = S>,
    {
        let owner = cx.owner().child();
        let scoped = cx.to_owned_cx().with_owner(owner.clone());
        if <V::Output as View>::REBUILDS_IN_PLACE {
            let content = Rc::new(RefCell::new(None));
            let sink = in_place(source, Rc::clone(&content), scoped);
            return Self {
                content: Content::InPlace(content),
                owner,
                updater: Some(Updater::Sink(sink)),
            };
        }
        let hole = Rc::new(RefCell::new(Hole::new(cx.dom())));
        let effect = in_hole(source, Rc::clone(&hole), scoped);
        Self {
            content: Content::Hole(hole),
            owner,
            updater: Some(Updater::Effect(effect)),
        }
    }

    /// Replaces the closure behind this hole, keeping the hole and its content where they are.
    ///
    /// A rebuilt closure captures values its predecessor did not, so the old updater is stopped and
    /// a new one takes over. The new updater's first run rebuilds the content in place, so no node
    /// moves and nothing flickers.
    fn restart<V>(&mut self, source: impl FnMut() -> V + 'static, cx: &mut BuildCx<'_>)
    where
        V: IntoView,
        V::Output: View<State = S>,
    {
        self.updater = None;
        let scoped = cx.to_owned_cx().with_owner(self.owner.clone());
        self.updater = Some(match &self.content {
            Content::InPlace(content) => {
                Updater::Sink(in_place(source, Rc::clone(content), scoped))
            }
            Content::Hole(hole) => Updater::Effect(in_hole(source, Rc::clone(hole), scoped)),
        });
    }
}

/// The effect that fills `hole` from `source`, in the scope of `scoped`.
///
/// A run builds the content if the hole is empty, and rebuilds the content that is there otherwise.
fn in_hole<V, S>(
    mut source: impl FnMut() -> V + 'static,
    hole: Rc<RefCell<Hole<S>>>,
    scoped: BuildCxOwned,
) -> RenderEffect<()>
where
    V: IntoView,
    V::Output: View<State = S>,
    S: Anchor,
{
    scoped.owner().clone().with(|| {
        RenderEffect::new(move |_| {
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
    })
}

/// The sink that keeps `content` up to date from `source`, in the scope of `scoped`.
///
/// Each run first cleans up the scope, so what the previous run created goes before this run
/// creates its own. The first run builds the content if there is none.
fn in_place<V, S>(
    mut source: impl FnMut() -> V + 'static,
    content: Rc<RefCell<Option<S>>>,
    scoped: BuildCxOwned,
) -> Sink
where
    V: IntoView,
    V::Output: View<State = S>,
    S: Anchor,
{
    scoped.owner().clone().with(|| {
        Sink::new(move || {
            scoped.owner().cleanup();
            let view = source().into_view();
            let mut cx = scoped.cx();
            let mut content = content.borrow_mut();
            match content.as_mut() {
                Some(existing) => view.rebuild(existing, &mut cx),
                None => *content = Some(view.build(&mut cx)),
            }
        })
    })
}

impl<S: Anchor> Anchor for ReactiveState<S> {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        match &self.content {
            Content::Hole(hole) => hole.borrow_mut().mount(dom, parent, before),
            Content::InPlace(content) => {
                if let Some(content) = content.borrow_mut().as_mut() {
                    content.mount(dom, parent, before);
                }
            }
        }
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.updater = None;
        match &self.content {
            Content::Hole(hole) => hole.borrow_mut().unmount(dom),
            Content::InPlace(content) => {
                if let Some(content) = content.borrow_mut().as_mut() {
                    content.unmount(dom);
                }
            }
        }
        self.owner.cleanup();
    }

    fn first_node(&self) -> Option<NodeId> {
        match &self.content {
            Content::Hole(hole) => hole.borrow().first_node(),
            Content::InPlace(content) => content.borrow().as_ref().and_then(Anchor::first_node),
        }
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

    #[test]
    fn a_text_hole_makes_one_node_and_one_sink() {
        use zgui_reactive::Sink;

        let f = Fixture::new();
        let count = f.window.with(|| RwSignal::new(0));
        let (nodes, sinks) = (f.backend.node_count(), Sink::live());

        let mut state = f
            .window
            .with(|| (move || count.get().to_string()).build(&mut f.cx()));
        state.mount(&f.dom, f.root, None);
        assert_eq!(f.backend.node_count(), nodes + 1, "no marker node");
        assert_eq!(Sink::live(), sinks + 1);
        let node = state.first_node().expect("the text node");
        assert!(!f.backend.is_marker(node));

        count.set(3);
        flush();
        assert_eq!(f.text(), "3");
        assert_eq!(state.first_node(), Some(node), "the node is kept");

        state.unmount(&f.dom);
        assert_eq!(Sink::live(), sinks, "unmounting frees the sink");
        f.window.unmount();
    }

    #[test]
    fn a_text_hole_cleans_up_each_run_before_the_next() {
        use std::cell::Cell;
        use std::rc::Rc;
        use zgui_reactive::on_cleanup_local;

        let f = Fixture::new();
        let count = f.window.with(|| RwSignal::new(0));
        let cleaned = Rc::new(Cell::new(0));
        let seen_at_start = Rc::new(Cell::new(None));
        let made: Rc<Cell<Option<RwSignal<i32>>>> = Rc::new(Cell::new(None));
        let (flag, keep, start) = (
            Rc::clone(&cleaned),
            Rc::clone(&made),
            Rc::clone(&seen_at_start),
        );

        let mut state = f.window.with(|| {
            (move || {
                start.set(Some(flag.get()));
                let flag = Rc::clone(&flag);
                on_cleanup_local(move || flag.set(flag.get() + 1));
                keep.set(Some(RwSignal::new(0)));
                count.get().to_string()
            })
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        let first = made.get().expect("the first run made a signal");
        assert_eq!(seen_at_start.get(), Some(0));

        count.set(1);
        flush();
        assert_eq!(
            seen_at_start.get(),
            Some(1),
            "the first run's cleanup ran before the second run started"
        );
        assert_eq!(cleaned.get(), 1);
        assert!(first.is_disposed(), "and what it made is gone");
        let second = made.get().expect("the second run made a signal");
        assert!(!second.is_disposed());

        state.unmount(&f.dom);
        assert_eq!(cleaned.get(), 2, "unmounting cleans up the last run");
        assert!(second.is_disposed());
        f.window.unmount();
    }

    #[test]
    fn a_text_hole_over_a_memo_and_its_source_runs_once_and_sees_them_agree() {
        use std::cell::RefCell;
        use std::rc::Rc;
        use zgui_reactive::Memo;

        let f = Fixture::new();
        let source = f.window.with(|| RwSignal::new(1));
        let doubled = f.window.with(|| Memo::new(move |_| source.get() * 2));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&seen);

        let mut state = f.window.with(|| {
            (move || {
                let pair = (source.get(), doubled.get());
                log.borrow_mut().push(pair);
                format!("{} {}", pair.0, pair.1)
            })
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);

        source.set(5);
        flush();
        assert_eq!(f.text(), "5 10");
        assert_eq!(
            *seen.borrow(),
            vec![(1, 2), (5, 10)],
            "one run per change, never torn"
        );

        state.unmount(&f.dom);
        f.window.unmount();
    }

    #[test]
    fn rebuilding_a_text_hole_keeps_its_node_and_follows_the_new_closure() {
        use std::cell::Cell;
        use std::rc::Rc;
        use zgui_reactive::on_cleanup_local;

        let f = Fixture::new();
        let (a, b) = f.window.with(|| (RwSignal::new(1), RwSignal::new(10)));
        let cleaned = Rc::new(Cell::new(false));
        let flag = Rc::clone(&cleaned);

        let mut state = f.window.with(|| {
            (move || {
                let flag = Rc::clone(&flag);
                on_cleanup_local(move || flag.set(true));
                a.get().to_string()
            })
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        let node = state.first_node();

        f.window
            .with(|| (move || b.get().to_string()).rebuild(&mut state, &mut f.cx()));
        assert_eq!(f.text(), "10");
        assert_eq!(state.first_node(), node, "the node is kept");
        assert!(cleaned.get(), "the old closure's run was cleaned up");

        a.set(2);
        b.set(20);
        flush();
        assert_eq!(f.text(), "20", "only the new closure drives the hole");

        state.unmount(&f.dom);
        f.window.unmount();
    }

    #[test]
    fn a_closure_whose_content_changes_shape_keeps_its_marker() {
        let f = Fixture::new();
        let shown = f.window.with(|| RwSignal::new(false));
        let after = f.dom.create_text("|");

        let mut state = f
            .window
            .with(|| (move || shown.get().then_some("x")).build(&mut f.cx()));
        state.mount(&f.dom, f.root, None);
        f.dom.insert(f.root, after, None);
        assert_eq!(f.text(), "|");
        let marker = state.first_node().expect("the marker");
        assert!(f.backend.is_marker(marker));

        shown.set(true);
        flush();
        assert_eq!(f.text(), "x|", "the content lands where the marker is");

        state.unmount(&f.dom);
        f.window.unmount();
    }
}
