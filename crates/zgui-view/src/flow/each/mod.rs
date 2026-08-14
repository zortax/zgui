//! A keyed list.

mod generation;
mod props;
mod reconcile;

pub use crate::flow::each::props::{ForProps, ForPropsBuilder};

use core::cell::RefCell;
use core::hash::Hash;
use core::marker::PhantomData;
use std::rc::Rc;

use zgui_reactive::{Owner, RenderEffect};

use crate::cx::{BuildCx, BuildCxOwned};
use crate::dom::DomHandle;
use crate::id::NodeId;
use crate::view::{Anchor, AnyView, AnyViewState, View};

use crate::flow::each::generation::Generations;
use crate::flow::each::reconcile::{Step, plan};

/// One row.
struct Item<K> {
    /// What identifies it.
    key: K,
    /// Its nodes.
    state: AnyViewState,
    /// Its own scope, disposed of when it goes away.
    owner: Owner,
    /// Which generation that scope belongs to.
    generation: usize,
}

/// Everything the list keeps between runs.
struct Rows<K> {
    /// The position marker every row is placed before.
    marker: NodeId,
    /// The parent, when mounted.
    parent: Option<NodeId>,
    /// The rows, in order.
    items: Vec<Item<K>>,
    /// The scopes the rows hang off.
    generations: Generations,
}

impl<K: Eq + Hash + Clone + 'static> Rows<K> {
    /// Rebuilds one surviving row through `build`, in the scope that row already has.
    ///
    /// The previous run of the row goes first, so a row that is rebuilt for as long as it is on
    /// screen holds one run's worth of signals, memos and cleanups rather than one per rebuild.
    fn refresh(
        item: &mut Item<K>,
        cx: &BuildCxOwned,
        build: &mut impl FnMut(&K, &BuildCx<'_>) -> AnyView,
    ) {
        let item_cx = cx.with_owner(item.owner.clone());
        let Item {
            key, state, owner, ..
        } = item;
        owner.with_cleanup(|| {
            build(key, &item_cx.cx()).rebuild(state, &mut item_cx.cx());
        });
    }

    /// Brings the rows into line with `keys`, building each new one with `build`.
    ///
    /// `refresh` says that `build` is a different closure from the one the rows on screen were made
    /// with, so every row that survives is built again through it.
    fn reconcile(
        &mut self,
        keys: Vec<K>,
        cx: &BuildCxOwned,
        refresh: bool,
        mut build: impl FnMut(&K, &BuildCx<'_>) -> AnyView,
    ) {
        let dom = cx.dom().clone();
        let old: Vec<K> = self.items.iter().map(|item| item.key.clone()).collect();
        let made = plan(&old, &keys);

        // Take every row out of the vector, so a plan step can put them back in its own order.
        let mut previous: Vec<Option<Item<K>>> = self.items.drain(..).map(Some).collect();
        for index in made.removed.iter().rev() {
            if let Some(mut item) = previous[*index].take() {
                item.state.unmount(&dom);
                item.owner.cleanup();
                self.generations.item_dropped(item.generation);
            }
        }

        let mut placed: Vec<Option<Item<K>>> = (0..keys.len()).map(|_| None).collect();
        let mut anchor = Some(self.marker);
        for position in (0..keys.len()).rev() {
            let item = match made.steps[position] {
                Step::Keep(index) => {
                    let mut item = previous[index]
                        .take()
                        .expect("a plan never reuses one row twice");
                    if refresh {
                        Self::refresh(&mut item, cx, &mut build);
                    }
                    item
                }
                Step::Move(index) => {
                    let mut item = previous[index]
                        .take()
                        .expect("a plan never reuses one row twice");
                    if refresh {
                        Self::refresh(&mut item, cx, &mut build);
                    }
                    if let Some(parent) = self.parent {
                        item.state.mount(&dom, parent, anchor);
                    }
                    item
                }
                Step::Create => {
                    let (generation, owner) = self.generations.scope_for_new_item();
                    let item_cx = cx.with_owner(owner.clone());
                    let key = keys[position].clone();
                    let mut state =
                        owner.with(|| build(&key, &item_cx.cx()).build(&mut item_cx.cx()));
                    if let Some(parent) = self.parent {
                        state.mount(&dom, parent, anchor);
                    }
                    Item {
                        key,
                        state,
                        owner,
                        generation,
                    }
                }
            };
            anchor = item.state.first_node().or(anchor);
            placed[position] = Some(item);
        }

        self.items = placed
            .into_iter()
            .map(|item| item.expect("every position was placed"))
            .collect();

        let live = u32::try_from(self.items.len()).unwrap_or(u32::MAX);
        self.generations.retire_if_needed(live);
    }
}

/// What a keyed list retains.
pub struct EachState<K> {
    /// The rows, shared with the effect that reconciles them.
    rows: Rc<RefCell<Rows<K>>>,
    /// The list's own scope.
    owner: Owner,
    /// The effect. Dropping it stops the list reconciling.
    effect: Option<RenderEffect<()>>,
}

impl<K: Eq + Hash + Clone + 'static> Anchor for EachState<K> {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        let mut rows = self.rows.borrow_mut();
        dom.insert(parent, rows.marker, before);
        rows.parent = Some(parent);
        let marker = rows.marker;
        for item in &mut rows.items {
            item.state.mount(dom, parent, Some(marker));
        }
    }

    fn unmount(&mut self, dom: &DomHandle) {
        self.effect = None;
        {
            let mut rows = self.rows.borrow_mut();
            for item in &mut rows.items {
                item.state.unmount(dom);
            }
            rows.items.clear();
            rows.generations.dispose();
            dom.detach(rows.marker);
            rows.parent = None;
        }
        self.owner.cleanup();
    }

    fn first_node(&self) -> Option<NodeId> {
        let rows = self.rows.borrow();
        rows.items
            .iter()
            .find_map(|item| item.state.first_node())
            .or(Some(rows.marker))
    }
}

/// The type parameters the three closures agree on, carried so that the list can name them.
type Agreed<I, T, K> = PhantomData<fn() -> (I, T, K)>;

/// A keyed list.
///
/// Each row gets its own scope, so removing row *k* frees exactly row *k*'s signals, memos and
/// cleanups, and does so before the call returns. The list's own dependency is the **key list
/// only** — the content of a row is read by that row's own bindings — so changing one row's data
/// re-runs that row's bindings and does not touch the list at all.
///
/// Rows are moved rather than rewritten. Inserting at the front of a thousand-row list is one node
/// insertion, not a thousand rewrites, which is the entire reason a keyed list exists.
///
/// # When `children` runs
///
/// Exactly twice as often as a caller usually needs to think about, and the two cases are the whole
/// contract:
///
/// * **A key the list has not seen** builds a row. A key that is already there keeps the row it
///   has, whatever the collection now says that row's data is. Give a row a signal when its content
///   has to follow its data, because the list itself watches the key list and nothing else.
/// * **A rebuild of the list** builds every row again. A rebuilt `For` carries a different
///   `children` — a component rebuilt with new props, a reactive hole that re-ran — and a row left
///   over from its predecessor would go on drawing what the predecessor said. This is the case a
///   list keyed by position depends on: its keys are `0..n` whatever the rows mean, so a change of
///   meaning moves no key and the rebuild is the only thing that reaches the rows.
///
/// A rebuilt row is rebuilt in its own scope, and the previous run of that scope is disposed of
/// first, exactly as a rebuilt component is.
///
/// # Panics
///
/// If two rows of one collection produce the same key. Which row is which then has no answer, so
/// the list says so rather than rendering one of them as nothing and moving the wrong one on the
/// next reorder.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, RwSignal, flush, install};
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::{Anchor, AnyView, BuildCxOwned, DocumentId, DomHandle, For, HostHandle, View};
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
/// let root = dom.create_element(ElementName::new("column"));
///
/// let rows = window.with(|| RwSignal::new(vec![1, 2, 3]));
/// let mut state = window.with(|| {
///     For::new(move || rows.get(), |row: &i32| *row, |row| AnyView::new(row.to_string()))
///         .build(&mut cx.cx())
/// });
/// state.mount(&dom, root, None);
/// assert_eq!(backend.text_content(root), "123");
///
/// rows.set(vec![3, 1]);
/// flush();
/// assert_eq!(backend.text_content(root), "31");
/// window.unmount();
/// ```
pub struct For<E, I, T, KF, K, VF> {
    /// Produces the collection, and is the list's only reactive dependency.
    each: E,
    /// Identifies a row.
    key: KF,
    /// Builds a row.
    children: VF,
    /// The types the closures agree on.
    types: Agreed<I, T, K>,
}

impl<E, I, T, KF, K, VF> For<E, I, T, KF, K, VF>
where
    E: Fn() -> I + 'static,
    I: IntoIterator<Item = T> + 'static,
    T: 'static,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + Clone + 'static,
    VF: Fn(T) -> AnyView + 'static,
{
    /// A list over `each`, identified by `key`, whose rows are built by `children`.
    pub fn new(each: E, key: KF, children: VF) -> Self {
        Self {
            each,
            key,
            children,
            types: PhantomData,
        }
    }
}

impl<E, I, T, KF, K, VF> View for For<E, I, T, KF, K, VF>
where
    E: Fn() -> I + 'static,
    I: IntoIterator<Item = T> + 'static,
    T: 'static,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + Clone + 'static,
    VF: Fn(T) -> AnyView + 'static,
{
    type State = EachState<K>;

    fn build(self, cx: &mut BuildCx<'_>) -> Self::State {
        let owner = cx.owner().child();
        let rows = Rc::new(RefCell::new(Rows {
            marker: cx.dom().create_marker(),
            parent: None,
            items: Vec::new(),
            generations: Generations::new(owner.clone()),
        }));
        let scoped = cx.to_owned_cx().with_owner(owner.clone());

        let effect = owner.with(|| RenderEffect::new(self.reconciler(Rc::clone(&rows), scoped)));
        EachState {
            rows,
            owner,
            effect: Some(effect),
        }
    }

    fn rebuild(self, state: &mut Self::State, cx: &mut BuildCx<'_>) {
        // A rebuilt list captures a different collection closure, so the effect that was watching
        // the old one is cancelled and a new one takes over the *same* rows. Nothing is unmounted:
        // the new effect's first run reconciles against what is already mounted, so a list whose
        // contents did not change moves no node.
        state.effect = None;
        let scoped = cx.to_owned_cx().with_owner(state.owner.clone());
        let rows = Rc::clone(&state.rows);
        state.effect = Some(
            state
                .owner
                .with(|| RenderEffect::new(self.reconciler(rows, scoped))),
        );
    }
}

impl<E, I, T, KF, K, VF> For<E, I, T, KF, K, VF>
where
    E: Fn() -> I + 'static,
    I: IntoIterator<Item = T> + 'static,
    T: 'static,
    KF: Fn(&T) -> K + 'static,
    K: Eq + Hash + Clone + 'static,
    VF: Fn(T) -> AnyView + 'static,
{
    /// The effect body: read the collection, and bring the rows into line with it.
    fn reconciler(
        self,
        rows: Rc<RefCell<Rows<K>>>,
        scoped: BuildCxOwned,
    ) -> impl FnMut(Option<()>) + 'static {
        let Self {
            each,
            key,
            children,
            ..
        } = self;
        move |previous: Option<()>| {
            // An effect runs for the first time when the list is built or rebuilt. A rebuild
            // carries a `children` that is not the closure the rows on screen were made with, so
            // that first run builds every row again; a later run is a changed key list and leaves
            // the rows it keeps alone.
            let refresh = previous.is_none();
            // The collection is read here, and it is the only thing this effect subscribes to.
            let items: Vec<T> = each().into_iter().collect();
            let keys: Vec<K> = items.iter().map(&key).collect();
            let mut sources: Vec<Option<T>> = items.into_iter().map(Some).collect();
            let mut by_key: std::collections::HashMap<K, usize> =
                std::collections::HashMap::with_capacity(keys.len());
            for (index, key) in keys.iter().enumerate() {
                by_key.entry(key.clone()).or_insert(index);
            }
            // Checked in every profile, and not only in a debug one: a check that is compiled out
            // of the build an application ships is a check for the one case that never happens.
            assert_eq!(
                by_key.len(),
                keys.len(),
                "two rows of a keyed list share a key, so which row is which has no answer: the \
                 second one renders as nothing and reordering them moves the wrong row. Give the \
                 key function something that distinguishes them."
            );

            rows.borrow_mut()
                .reconcile(keys.clone(), &scoped, refresh, |key, _| {
                    match by_key.get(key).and_then(|index| sources[*index].take()) {
                        Some(item) => children(item),
                        None => AnyView::new(()),
                    }
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use zgui_reactive::prelude::*;
    use zgui_reactive::{RwSignal, flush, on_cleanup_local};

    use super::For;
    use crate::fixture::Fixture;
    use crate::view::{Anchor, AnyView, View};

    #[test]
    fn rows_are_built_once_and_reordered_rather_than_rewritten() {
        let f = Fixture::new();
        let rows = f.window.with(|| RwSignal::new(vec![1, 2, 3]));
        let mut state = f.window.with(|| {
            For::new(
                move || rows.get(),
                |row: &i32| *row,
                |row| AnyView::new(row.to_string()),
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        assert_eq!(f.text(), "123");
        let nodes_after_build = f.backend.node_count();

        rows.set(vec![3, 2, 1]);
        flush();
        assert_eq!(f.text(), "321");
        assert_eq!(
            f.backend.node_count(),
            nodes_after_build,
            "a reorder creates no nodes"
        );
        f.window.unmount();
    }

    #[test]
    fn removing_a_row_disposes_of_that_rows_scope_and_no_others() {
        let f = Fixture::new();
        let cleaned = Rc::new(Cell::new(0));
        let rows = f.window.with(|| RwSignal::new(vec![1, 2, 3]));
        let counter = Rc::clone(&cleaned);

        let mut state = f.window.with(|| {
            For::new(
                move || rows.get(),
                |row: &i32| *row,
                move |row| {
                    let counter = Rc::clone(&counter);
                    AnyView::new(move || {
                        let counter = Rc::clone(&counter);
                        on_cleanup_local(move || counter.set(counter.get() + 1));
                        row.to_string()
                    })
                },
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        assert_eq!(cleaned.get(), 0);

        rows.set(vec![1, 3]);
        flush();
        assert_eq!(f.text(), "13");
        assert_eq!(cleaned.get(), 1, "exactly the removed row was disposed of");
        f.window.unmount();
    }

    #[test]
    #[should_panic(expected = "share a key")]
    fn two_rows_with_the_same_key_are_reported_rather_than_rendered_wrong() {
        // Without the check the second row renders as nothing and a later reorder moves whichever
        // of the two the map happened to name — a wrong picture with no error anywhere.
        let f = Fixture::new();
        let rows = f.window.with(|| RwSignal::new(vec![1, 1]));
        let mut state = f.window.with(|| {
            For::new(
                move || rows.get(),
                |row: &i32| *row,
                |row| AnyView::new(row.to_string()),
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
    }

    #[test]
    fn an_empty_list_still_knows_where_it_is() {
        let f = Fixture::new();
        let rows = f.window.with(|| RwSignal::new(Vec::<i32>::new()));
        let mut state = f.window.with(|| {
            For::new(
                move || rows.get(),
                |row: &i32| *row,
                |row| AnyView::new(row.to_string()),
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        f.dom.insert(f.root, f.dom.create_text("|"), None);
        assert_eq!(f.text(), "|");

        rows.set(vec![7]);
        flush();
        assert_eq!(f.text(), "7|");
        f.window.unmount();
    }

    /// A rebuilt list carries a new row builder, and every row has to come from it.
    ///
    /// A window onto a longer list keys its rows by position, so a list that changed what a row
    /// means while the window stood still would keep drawing the previous builder's rows.
    #[test]
    fn a_rebuilt_list_builds_every_row_through_the_new_builder() {
        let f = Fixture::new();
        let rows = f.window.with(|| RwSignal::new(vec![1, 2, 3]));
        let mut state = f.window.with(|| {
            For::new(
                move || rows.get(),
                |row: &i32| *row,
                |row| AnyView::new(format!("a{row}")),
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        assert_eq!(f.text(), "a1a2a3");

        f.window.with(|| {
            For::new(
                move || rows.get(),
                |row: &i32| *row,
                |row| AnyView::new(format!("b{row}")),
            )
            .rebuild(&mut state, &mut f.cx());
        });
        assert_eq!(
            f.text(),
            "b1b2b3",
            "the kept rows came from the old builder"
        );
        f.window.unmount();
    }

    /// The other half of the same contract, and the reason a keyed list is worth having: a changed
    /// collection builds the keys that are new to the list and leaves every other row where it is.
    #[test]
    fn a_changed_collection_builds_only_the_keys_that_are_new() {
        let f = Fixture::new();
        let built = Rc::new(Cell::new(0));
        let counter = Rc::clone(&built);
        let rows = f.window.with(|| RwSignal::new(vec![1, 2]));

        let mut state = f.window.with(|| {
            For::new(
                move || rows.get(),
                |row: &i32| *row,
                move |row: i32| {
                    counter.set(counter.get() + 1);
                    AnyView::new(row.to_string())
                },
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        assert_eq!(built.get(), 2);

        rows.set(vec![2, 1, 3]);
        flush();
        assert_eq!(f.text(), "213");
        assert_eq!(
            built.get(),
            3,
            "a reorder rebuilt a row that was already there"
        );
        f.window.unmount();
    }

    /// A row that is rebuilt over and over holds one run of itself, the way a rebuilt component
    /// does. Without the disposal a list under a reactive hole would accumulate one row's worth of
    /// signals, memos and cleanups per re-run, for as long as it stayed mounted.
    #[test]
    fn a_rebuilt_row_disposes_of_its_previous_run() {
        let f = Fixture::new();
        let live = Rc::new(Cell::new(0i32));
        let rows = f.window.with(|| RwSignal::new(vec![1, 2, 3]));
        let builder = {
            let live = Rc::clone(&live);
            move |row: i32| {
                let live = Rc::clone(&live);
                live.set(live.get() + 1);
                on_cleanup_local(move || live.set(live.get() - 1));
                AnyView::new(row.to_string())
            }
        };

        let mut state = f.window.with(|| {
            For::new(move || rows.get(), |row: &i32| *row, builder.clone()).build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);
        assert_eq!(live.get(), 3);

        for _ in 0..8 {
            let builder = builder.clone();
            f.window.with(|| {
                For::new(move || rows.get(), |row: &i32| *row, builder)
                    .rebuild(&mut state, &mut f.cx());
            });
            assert_eq!(live.get(), 3, "one run of each row is alive at a time");
        }

        state.unmount(&f.dom);
        assert_eq!(live.get(), 0);
        f.window.unmount();
    }

    #[test]
    fn unmounting_the_list_disposes_of_every_row() {
        let f = Fixture::new();
        let cleaned = Rc::new(Cell::new(0));
        let counter = Rc::clone(&cleaned);
        let rows = f.window.with(|| RwSignal::new(vec![1, 2, 3]));

        let mut state = f.window.with(|| {
            For::new(
                move || rows.get(),
                |row: &i32| *row,
                move |row| {
                    let counter = Rc::clone(&counter);
                    AnyView::new(move || {
                        let counter = Rc::clone(&counter);
                        on_cleanup_local(move || counter.set(counter.get() + 1));
                        row.to_string()
                    })
                },
            )
            .build(&mut f.cx())
        });
        state.mount(&f.dom, f.root, None);

        state.unmount(&f.dom);
        assert_eq!(cleaned.get(), 3);
        assert_eq!(f.text(), "");
        f.window.unmount();
    }
}
