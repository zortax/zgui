//! An ordered set of items, in the order a reader meets them.

mod item;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};

pub use crate::collection::item::{CollectionItem, ItemId};

/// The items of one composite control, kept in tree order.
///
/// A menu, a listbox and a tab bar all need the same thing: the set of their own items, in the
/// order they appear, so that the down-arrow key means *the next one* and the typeahead means *the
/// next one whose label starts with this*. Nothing else in a retained tree answers that — a parent
/// does not enumerate its children, and its items may be behind a conditional, inside a list, or
/// three components down.
///
/// So the items announce themselves. Registration is opt-in and per component, which is also what
/// keeps it honest: a decorative separator is not an item, and nothing here has to guess.
///
/// **The order is tree order, not registration order.** A keyed list rebuilds only the rows whose
/// keys moved, so a menu whose items came from one would otherwise answer the down-arrow with
/// whichever row was last rebuilt.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui_primitives::Collection;
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     // The parent publishes one; every item below reaches it with `Collection::current`.
///     let collection = Collection::provide();
///     assert_eq!(collection.len(), 0);
///     assert!(Collection::current().is_some());
/// });
/// scope.unmount();
/// ```
#[derive(Copy, Clone)]
pub struct Collection {
    /// Everything registered, in registration order.
    items: RwSignal<Vec<CollectionItem>, LocalStorage>,
    /// The next number to hand out.
    next: RwSignal<u64, LocalStorage>,
}

impl Collection {
    /// An empty collection belonging to the current scope.
    pub fn new() -> Self {
        Self {
            items: RwSignal::new_local(Vec::new()),
            next: RwSignal::new_local(1),
        }
    }

    /// Creates one and publishes it to every scope below this one.
    pub fn provide() -> Self {
        let collection = Self::new();
        provide_local_context(collection);
        collection
    }

    /// The collection the nearest enclosing parent published, when there is one.
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Registers `node` as an item, and takes it out again when the calling scope goes away.
    ///
    /// The handle is the item's name in every other call here. An item registered twice is two
    /// items, which is a component calling this from somewhere that re-runs.
    pub fn register(&self, node: NodeRef) -> ItemId {
        self.register_reachable_when(node, Signal::stored_local(true))
    }

    /// The same, for an item the arrow keys should skip while `reachable` reads false.
    ///
    /// The item is still *in* the collection — it keeps its place, and a reader still meets it —
    /// but [`Collection::step`] passes over it. That is the difference between a control that is
    /// disabled and one that is not there: a disabled tab is announced, and arrowing onto it would
    /// strand the keyboard on something that refuses to be chosen.
    pub fn register_reachable_when(
        &self,
        node: NodeRef,
        reachable: Signal<bool, LocalStorage>,
    ) -> ItemId {
        let id = ItemId::new(self.next.get_untracked());
        self.next.set(id.get() + 1);
        self.items.update(|items| {
            items.push(CollectionItem::reachable_when(id, node, reachable));
        });

        let collection = *self;
        on_cleanup_local(move || collection.deregister(id));
        id
    }

    /// Takes an item out.
    pub fn deregister(&self, id: ItemId) {
        self.items
            .try_update(|items| items.retain(|item| item.id() != id));
    }

    /// Every item, in tree order.
    ///
    /// Subscribes to the set, so a view built from this rebuilds when an item comes or goes.
    ///
    /// A collection that is gone holds no item. An item can outlive its parent by a moment — a
    /// menu that closes disposes the collection while its items are still coming down, and a
    /// binding of one of them can run once more as it goes — so the answer then is an empty set
    /// rather than a panic.
    pub fn items(&self) -> Vec<CollectionItem> {
        Self::in_tree_order(self.items.try_get().unwrap_or_default())
    }

    /// The same, without subscribing.
    pub fn items_untracked(&self) -> Vec<CollectionItem> {
        Self::in_tree_order(self.items.try_get_untracked().unwrap_or_default())
    }

    /// How many items there are.
    pub fn len(&self) -> usize {
        self.items.try_get().unwrap_or_default().len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Where `id` sits in tree order, when it is still registered.
    pub fn index_of(&self, id: ItemId) -> Option<usize> {
        self.items_untracked()
            .iter()
            .position(|item| item.id() == id)
    }

    /// The item at `index` in tree order.
    pub fn at(&self, index: usize) -> Option<CollectionItem> {
        self.items_untracked().get(index).copied()
    }

    /// The item `steps` reachable positions after `from`, wrapping when asked to.
    ///
    /// Items that are not reachable — a disabled tab, say — are passed over rather than landed on,
    /// so one arrow key is always one *usable* move. `None` when there are no items, when none of
    /// them can be reached, or when the end was reached and wrapping was refused, which is what an
    /// arrow key at the end of a non-cycling list means.
    pub fn step(&self, from: Option<ItemId>, steps: isize, wrap: bool) -> Option<CollectionItem> {
        let items = self.items_untracked();
        if items.is_empty() {
            return None;
        }
        let length = items.len() as isize;
        let mut at = from
            .and_then(|id| items.iter().position(|item| item.id() == id))
            .map_or(if steps >= 0 { -1 } else { length }, |at| at as isize);
        let direction = if steps >= 0 { 1 } else { -1 };
        let mut landed = None;

        for _ in 0..steps.unsigned_abs().max(1) {
            // One pass over the collection at most: past that every item has been offered and
            // refused, and a second lap would spin for ever on a group where nothing is reachable.
            let mut found = None;
            for _ in 0..length {
                let next = at + direction;
                let next = if wrap {
                    next.rem_euclid(length)
                } else if next < 0 || next >= length {
                    return None;
                } else {
                    next
                };
                at = next;
                if items[at as usize].is_reachable() {
                    found = Some(items[at as usize]);
                    break;
                }
            }
            landed = Some(found?);
        }
        landed
    }

    /// The first item the keyboard may land on, or the last.
    ///
    /// What <kbd>Home</kbd> and <kbd>End</kbd> mean in a group: the ends of what can actually be
    /// reached, not the ends of the list, so a group whose first entry is disabled still answers
    /// <kbd>Home</kbd> with something usable.
    pub fn end(&self, last: bool) -> Option<CollectionItem> {
        let items = self.items_untracked();
        let mut reachable = items.iter().filter(|item| item.is_reachable());
        if last {
            reachable.next_back().copied()
        } else {
            reachable.next().copied()
        }
    }

    /// Puts a registration list back into the order a reader meets it in.
    ///
    /// The comparison asks the engine where each node sits, which is the only authority on it: the
    /// order items were built in is the order the last rebuild happened to take, and a list that
    /// has reordered its rows since then has neither.
    fn in_tree_order(mut items: Vec<CollectionItem>) -> Vec<CollectionItem> {
        // An item whose node is gone sorts to the end and is dropped: it is on its way out, and a
        // comparison against a node that no longer exists has no answer.
        items.retain(|item| item.node().is_bound());
        // A stable sort over a comparison the engine answers — asked in *both* directions. The
        // engine refuses to order some pairs it can still see: a node detached mid-frame during a
        // Presence exit is bound, but sits in no tree that contains the other one. Reading that
        // refusal as Greater said "left after right" from both sides at once, which is not an
        // order at all and is exactly the inconsistency the standard sort detects and panics on.
        // Two refusals are an honest Equal, and a stable sort leaves such a pair in the order it
        // was already in rather than moving it arbitrarily.
        items.sort_by(|left, right| {
            let (Some(first), Some(second)) =
                (left.node().get_untracked(), right.node().get_untracked())
            else {
                return core::cmp::Ordering::Equal;
            };
            if left.node().precedes(second) {
                core::cmp::Ordering::Less
            } else if right.node().precedes(first) {
                core::cmp::Ordering::Greater
            } else {
                core::cmp::Ordering::Equal
            }
        });
        items
    }
}

impl Default for Collection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use zgui::prelude::*;
    use zgui::reactive::{Mounted, RwSignal, install};
    use zgui_testkit_view::Window;

    use super::Collection;

    /// Three bound handles, in the order the tree holds them.
    fn three(window: &Window) -> Vec<NodeRef> {
        use zgui::view::Dom;
        (0..3)
            .map(|_| {
                let node = window
                    .dom
                    .create_element(zgui::view::ElementName::new("control"));
                window.dom.insert(window.root, node, None);
                let handle = window.scope.with(NodeRef::new);
                handle.bind(node, &window.dom_handle, &window.host_handle);
                handle
            })
            .collect()
    }

    #[test]
    fn items_come_back_in_tree_order_however_they_registered() {
        let window = Window::open();
        let handles = three(&window);
        let nodes: Vec<_> = handles
            .iter()
            .map(|handle| handle.get_untracked().expect("bound"))
            .collect();
        window.host.set_tree_order(nodes.clone());

        let collection = window.scope.with(Collection::new);
        // Registered back to front, exactly as a keyed list that has just reversed itself does.
        for handle in handles.iter().rev() {
            window.scope.with(|| collection.register(*handle));
        }

        let ordered: Vec<_> = collection
            .items_untracked()
            .iter()
            .map(|item| item.node().get_untracked().expect("bound"))
            .collect();
        assert_eq!(ordered, nodes, "registration order is not tree order");
    }

    #[test]
    fn a_node_the_engine_cannot_order_compares_equal_from_both_sides() {
        // The host refuses to order a node it was never told the place of, which is the same
        // answer the real engine gives for a node detached mid-frame during a Presence exit: bound,
        // visible, in no tree the others are in. The old comparator read every refusal as Greater,
        // so such a node sorted after its neighbour *from both directions* — the total-order
        // violation the standard sort panics on. What is wanted instead: the sort finishes, keeps
        // every item, and still orders the pair the engine can order.
        let window = Window::open();
        let handles = three(&window);
        let nodes: Vec<_> = handles
            .iter()
            .map(|handle| handle.get_untracked().expect("bound"))
            .collect();
        window.host.set_tree_order(vec![nodes[0], nodes[2]]);

        let collection = window.scope.with(Collection::new);
        // The orderable pair registered back to front, so the sort has real work to do with the
        // unorderable node in its way.
        for handle in [handles[2], handles[0], handles[1]] {
            window.scope.with(|| collection.register(handle));
        }

        let ordered: Vec<_> = collection
            .items_untracked()
            .iter()
            .map(|item| item.node().get_untracked().expect("bound"))
            .collect();
        assert_eq!(ordered.len(), 3, "no item was lost to the refusal");
        let place = |node| {
            ordered
                .iter()
                .position(|candidate| *candidate == node)
                .expect("kept")
        };
        assert!(
            place(nodes[0]) < place(nodes[2]),
            "the pair the engine can order still came back in tree order"
        );
    }

    #[test]
    fn an_item_leaves_the_collection_with_the_scope_that_registered_it() {
        let window = Window::open();
        let handles = three(&window);
        let collection = window.scope.with(Collection::new);

        let first = window.scope.with(Mounted::new);
        first.with(|| collection.register(handles[0]));
        window.scope.with(|| collection.register(handles[1]));
        assert_eq!(collection.len(), 2);

        first.unmount();
        assert_eq!(collection.len(), 1);
    }

    #[test]
    fn stepping_wraps_when_asked_and_stops_when_not() {
        let window = Window::open();
        let handles = three(&window);
        let nodes: Vec<_> = handles
            .iter()
            .map(|handle| handle.get_untracked().expect("bound"))
            .collect();
        window.host.set_tree_order(nodes);

        let collection = window.scope.with(Collection::new);
        let ids: Vec<_> = handles
            .iter()
            .map(|handle| window.scope.with(|| collection.register(*handle)))
            .collect();

        assert_eq!(
            collection.step(None, 1, true).map(|item| item.id()),
            Some(ids[0])
        );
        assert_eq!(
            collection.step(Some(ids[2]), 1, true).map(|item| item.id()),
            Some(ids[0]),
            "wrapping goes round"
        );
        assert!(
            collection.step(Some(ids[2]), 1, false).is_none(),
            "and not wrapping stops"
        );
        assert_eq!(
            collection
                .step(Some(ids[0]), -1, true)
                .map(|item| item.id()),
            Some(ids[2])
        );
    }

    #[test]
    fn an_item_the_keyboard_may_not_land_on_is_stepped_over_and_is_not_an_end() {
        // Not removed: it keeps its place, so the set a reader is told about is the set that is
        // there. What changes is where one arrow key lands, and where Home and End land.
        let window = Window::open();
        let handles = three(&window);
        let nodes: Vec<_> = handles
            .iter()
            .map(|handle| handle.get_untracked().expect("bound"))
            .collect();
        window.host.set_tree_order(nodes);

        let collection = window.scope.with(Collection::new);
        let blocked = window.scope.with(|| RwSignal::new_local(true));
        let ids: Vec<_> = handles
            .iter()
            .enumerate()
            .map(|(at, handle)| {
                window.scope.with(|| {
                    if at == 1 {
                        collection.register_reachable_when(
                            *handle,
                            Signal::derive_local(move || !blocked.get()),
                        )
                    } else {
                        collection.register(*handle)
                    }
                })
            })
            .collect();

        assert_eq!(collection.len(), 3, "the item is still in the set");
        assert_eq!(
            collection.step(Some(ids[0]), 1, true).map(|item| item.id()),
            Some(ids[2]),
            "the arrow key stopped on the item it cannot use"
        );
        assert_eq!(
            collection
                .step(Some(ids[2]), -1, true)
                .map(|item| item.id()),
            Some(ids[0]),
            "and backwards too"
        );
        assert_eq!(collection.end(true).map(|item| item.id()), Some(ids[2]));

        // The same collection once the item can be reached again: nothing was thrown away.
        blocked.set(false);
        assert_eq!(
            collection.step(Some(ids[0]), 1, true).map(|item| item.id()),
            Some(ids[1])
        );
    }

    #[test]
    fn a_group_where_nothing_can_be_reached_answers_every_key_with_nothing() {
        let window = Window::open();
        let handles = three(&window);
        let collection = window.scope.with(Collection::new);
        for handle in &handles {
            window
                .scope
                .with(|| collection.register_reachable_when(*handle, Signal::stored_local(false)));
        }

        assert!(
            collection.step(None, 1, true).is_none(),
            "and it terminates"
        );
        assert!(collection.step(None, -1, true).is_none());
        assert!(collection.end(false).is_none());
        assert!(collection.end(true).is_none());
    }

    #[test]
    fn a_collection_that_is_gone_holds_no_item() {
        // An item outlives its parent by a moment: a menu that closes disposes the collection
        // while the items are still coming down, and a binding of one of them — the `tabindex` of
        // a roving item, say — can run once more on its way out. `RovingContext::active` already
        // answers "nobody" for a group that is gone, and every reader here has to agree with it.
        let window = Window::open();
        let handles = three(&window);
        let parent = window.scope.with(Mounted::new);
        let collection = parent.with(Collection::new);
        for handle in &handles {
            parent.with(|| collection.register(*handle));
        }
        assert_eq!(collection.len(), 3);

        parent.unmount();

        assert!(collection.items().is_empty());
        assert!(collection.items_untracked().is_empty());
        assert_eq!(collection.len(), 0);
        assert!(collection.is_empty());
        assert!(collection.end(false).is_none());
        assert!(collection.step(None, 1, true).is_none());
        assert!(collection.at(0).is_none());
    }

    #[test]
    fn a_collection_is_reached_through_the_scope_and_not_a_global() {
        install().ok();
        let outside = Mounted::new();
        assert!(outside.with(Collection::current).is_none());

        let parent = Mounted::new();
        let published = parent.with(Collection::provide);
        let child = parent.with(Mounted::new);
        let found = child.with(Collection::current).expect("published above");
        assert_eq!(found.len(), published.len());

        outside.unmount();
        parent.unmount();
    }
}
