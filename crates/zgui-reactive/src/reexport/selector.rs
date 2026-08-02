//! The evicting selector.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use reactive_graph::owner::Owner;

use crate::own::on_cleanup_local;

/// "Which one of these is selected?", answered in constant time per row.
///
/// The naive spelling — every row reading a shared `selected` signal and comparing it to its own
/// key — wakes every row on every change, so selecting a different row in a thousand-row list
/// re-runs a thousand bindings. A selector inverts that: it keeps one small signal per key that
/// is being watched and, when the source changes, notifies only the key that was selected and
/// the key that now is. Two rows re-run instead of a thousand.
///
/// Entries are evicted with the scopes that asked for them. A row that reads
/// [`is_selected`](Selector::is_selected) registers a cleanup in the current scope, so unmounting
/// the row removes its entry; a key watched by nothing costs nothing. Without that, every key
/// ever displayed stays in the map for the lifetime of the list, and the update pass walks all
/// of them.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, RwSignal, Scope, Selector, install};
///
/// install().unwrap();
/// let root = Mounted::new();
/// let (selected, selector, rows) = root.with(|| {
///     let selected = RwSignal::new(0_usize);
///     (selected, Selector::new(move || selected.get()), Scope::new())
/// });
///
/// let row = rows.mount();
/// assert!(row.with(|| selector.is_selected(&0)));
/// assert!(!row.with(|| selector.is_selected(&1)));
/// assert_eq!(selector.watched_keys(), 2);
///
/// row.unmount();
/// assert_eq!(selector.watched_keys(), 0);
///
/// selected.set(1);
/// root.unmount();
/// ```
#[derive(Clone)]
pub struct Selector<K>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
{
    /// The wrapped selector, which never removes an entry by itself.
    inner: reactive_graph::computed::Selector<K>,
    /// Which scopes are watching which keys, and how many scopes each key has.
    watchers: Rc<RefCell<Watchers<K>>>,
}

/// The bookkeeping that decides when a key stops being watched.
struct Watchers<K> {
    /// Scopes registered per key, so a key shared by two rows outlives the first one.
    counts: HashMap<K, usize>,
    /// The `(scope, key)` pairs already registered, so re-reading a key in the same scope does
    /// not register a second cleanup.
    registered: std::collections::HashSet<(usize, K)>,
}

impl<K> Selector<K>
where
    K: Eq + Hash + Clone + Send + Sync + 'static,
{
    /// Creates a selector over `source`, comparing keys for equality.
    ///
    /// `source` is read reactively, so the selector follows whatever signals it touches.
    #[must_use]
    pub fn new(source: impl Fn() -> K + Clone + Send + Sync + 'static) -> Self {
        Self::new_with_fn(source, |key, current| key == current)
    }

    /// Creates a selector over `source`, deciding "is this key the selected one?" with `matches`.
    ///
    /// For keys that are not compared by equality — a path that is selected when it is a prefix
    /// of the current one, a range that contains it.
    #[must_use]
    pub fn new_with_fn(
        source: impl Fn() -> K + Clone + Send + Sync + 'static,
        matches: impl Fn(&K, &K) -> bool + Clone + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: reactive_graph::computed::Selector::new_with_fn(source, matches),
            watchers: Rc::new(RefCell::new(Watchers {
                counts: HashMap::new(),
                registered: std::collections::HashSet::new(),
            })),
        }
    }

    /// Whether `key` is the selected one, subscribing the current scope to changes for `key`
    /// alone.
    ///
    /// Call it from the row it belongs to. The entry it creates is removed when that row's scope
    /// is disposed of; with no current owner nothing can be evicted, so the entry is kept and
    /// the caller is asked, in debug builds, to provide a scope.
    pub fn is_selected(&self, key: &K) -> bool {
        self.watch(key);
        self.inner.selected(key)
    }

    /// How many keys currently have an entry.
    ///
    /// Diagnostic. It is the number that grows without bound if entries are never evicted, and
    /// the one a long-running list's tests should assert on.
    #[must_use]
    pub fn watched_keys(&self) -> usize {
        self.watchers.borrow().counts.len()
    }

    /// Registers the current scope as a watcher of `key`, once.
    fn watch(&self, key: &K) {
        let Some(owner) = Owner::current() else {
            crate::executor::assert_owner("Selector::is_selected");
            return;
        };
        let scope = owner.debug_id();

        {
            let mut watchers = self.watchers.borrow_mut();
            if !watchers.registered.insert((scope, key.clone())) {
                return;
            }
            *watchers.counts.entry(key.clone()).or_insert(0) += 1;
        }

        let key = key.clone();
        let inner = self.inner.clone();
        let watchers = Rc::clone(&self.watchers);
        on_cleanup_local(move || {
            let mut watchers = watchers.borrow_mut();
            watchers.registered.remove(&(scope, key.clone()));
            if let Some(count) = watchers.counts.get_mut(&key) {
                *count -= 1;
                if *count == 0 {
                    watchers.counts.remove(&key);
                    inner.remove(&key);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use reactive_graph::signal::RwSignal;
    use reactive_graph::traits::{Get, Set};

    use super::*;
    use crate::executor::{flush, install};
    use crate::own::{Mounted, Scope};

    /// Builds a selector over a signal, in a root scope.
    fn fixture() -> (Mounted, RwSignal<usize>, Selector<usize>) {
        install().unwrap();
        // Every `is_selected` below is read from a test rather than from a binding, which is
        // exactly the shape the "read outside a tracking context" diagnostic exists to catch.
        std::mem::forget(crate::zone::enter_non_reactive_zone());
        let root = Mounted::new();
        let (source, selector) = root.with(|| {
            let source = RwSignal::new(0);
            (source, Selector::new(move || source.get()))
        });
        (root, source, selector)
    }

    #[test]
    fn ten_thousand_rows_leave_nothing_behind() {
        let (root, _source, selector) = fixture();
        let rows = root.with(Scope::new);

        for key in 0..10_000 {
            let row = rows.mount();
            row.with(|| selector.is_selected(&key));
            row.unmount();
        }

        assert_eq!(selector.watched_keys(), 0);
        root.unmount();
    }

    #[test]
    fn a_key_two_rows_watch_survives_the_first_of_them() {
        let (root, _source, selector) = fixture();
        let rows = root.with(Scope::new);

        let first = rows.mount();
        let second = rows.mount();
        first.with(|| selector.is_selected(&7));
        second.with(|| selector.is_selected(&7));
        assert_eq!(selector.watched_keys(), 1);

        first.unmount();
        assert_eq!(selector.watched_keys(), 1);
        second.unmount();
        assert_eq!(selector.watched_keys(), 0);
        root.unmount();
    }

    #[test]
    fn reading_a_key_twice_in_one_scope_registers_it_once() {
        let (root, _source, selector) = fixture();
        let rows = root.with(Scope::new);

        let row = rows.mount();
        row.with(|| {
            assert!(selector.is_selected(&0));
            assert!(selector.is_selected(&0));
        });
        assert_eq!(selector.watched_keys(), 1);

        row.unmount();
        assert_eq!(selector.watched_keys(), 0);
        root.unmount();
    }

    #[test]
    fn only_the_two_affected_rows_re_run() {
        let (root, source, selector) = fixture();
        let rows = root.with(Scope::new);
        let runs = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut mounted = Vec::new();

        for key in 0..8 {
            let row = rows.mount();
            let effect = row.with(|| {
                let runs = std::rc::Rc::clone(&runs);
                let selector = selector.clone();
                reactive_graph::effect::RenderEffect::new(move |_| {
                    selector.is_selected(&key);
                    runs.set(runs.get() + 1);
                })
            });
            mounted.push((row, effect));
        }
        assert_eq!(runs.get(), 8, "one first run per row");

        source.set(3);
        flush();
        assert_eq!(
            runs.get(),
            10,
            "the row that lost it and the row that gained it"
        );

        root.unmount();
    }
}
