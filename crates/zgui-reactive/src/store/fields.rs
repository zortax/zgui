//! Keyed collections whose field signals exist only while a reader watches them.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::rc::Rc;

use reactive_graph::graph::Observer;
use reactive_graph::owner::{LocalStorage, Owner, StoredValue};
use reactive_graph::signal::ArcTrigger;
use reactive_graph::traits::{Notify, ReadValue, Track};
use smallvec::SmallVec;

use crate::own::on_cleanup_local;

/// The slot that stands for the whole value of a key.
const WHOLE: u8 = u8::MAX;

/// The fewest unwatched signals that are kept before a sweep drops them.
const IDLE_MIN: usize = 64;

/// One field of `T`, as [`KeyedFields`] reads and writes it.
///
/// A lens is two plain functions and a slot number. The slot identifies the field: give each field
/// of a collection its own slot, from 0 to 254. Declare lenses as constants next to the type.
///
/// ```
/// use zgui_reactive::Lens;
///
/// /// One record.
/// struct Record {
///     /// What it is called.
///     name: String,
///     /// How many there are.
///     qty: u32,
/// }
///
/// /// The name field.
/// const NAME: Lens<Record, String> = Lens::new(0, |r| &r.name, |r| &mut r.name);
/// /// The quantity field.
/// const QTY: Lens<Record, u32> = Lens::new(1, |r| &r.qty, |r| &mut r.qty);
/// # let _ = (NAME, QTY);
/// ```
pub struct Lens<T, V> {
    /// Which field this is.
    slot: u8,
    /// Reads the field.
    read: fn(&T) -> &V,
    /// Writes the field.
    write: fn(&mut T) -> &mut V,
}

impl<T, V> Lens<T, V> {
    /// Makes a lens for the field in `slot`, read by `read` and written by `write`.
    ///
    /// # Panics
    ///
    /// If `slot` is 255. That slot stands for the whole value.
    #[must_use]
    pub const fn new(slot: u8, read: fn(&T) -> &V, write: fn(&mut T) -> &mut V) -> Self {
        assert!(
            slot != WHOLE,
            "lens slot 255 is reserved for the whole value"
        );
        Self { slot, read, write }
    }

    /// The slot this lens was made with.
    #[must_use]
    pub const fn slot(&self) -> u8 {
        self.slot
    }
}

impl<T, V> Clone for Lens<T, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T, V> Copy for Lens<T, V> {}

impl<T, V> fmt::Debug for Lens<T, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lens").field("slot", &self.slot).finish()
    }
}

/// A keyed collection of plain values, with one signal per field that a reader watches.
///
/// A collection of records with one [`RwSignal`](crate::RwSignal) per field costs a few hundred
/// bytes per field, for each record, from the start. Most of those signals are never read when a
/// list shows only the rows in its viewport. `KeyedFields` holds the records as plain values. A
/// field gets a signal when a reader first reads it, and loses it after the last owner that read
/// it is disposed of. A record that no view shows costs only its value and its map entry.
///
/// Reads through [`get`](Self::get) and [`with`](Self::with) subscribe the reader to one field of
/// one key. A write through [`set`](Self::set) or [`update`](Self::update) notifies the readers of
/// that field and the readers of the whole value. [`insert`](Self::insert),
/// [`remove`](Self::remove) and [`update_value`](Self::update_value) notify every reader of the
/// key. A reader that has no owner keeps its subscription only until the next sweep.
///
/// An owner keeps each field it read until it is disposed of, or until it runs again as an effect
/// or a memo. A [`Sink`](crate::Sink) runs again under its creation owner, so a binding that
/// starts to read a different key keeps the old key's signals until its row unmounts. Key the
/// rows by record, for example with a keyed `for` over the keys, so that a row reads one key for
/// its whole life.
///
/// The handle is `Copy`, belongs to the owner that was current when it was made, and holds nothing
/// after that owner is disposed of. Read and write it on the UI thread.
///
/// # A big list
///
/// Keep the order as a signal of keys, and the records in a `KeyedFields`. Each row reads its own
/// fields by key. Mounted rows hold one signal per field they show. The other records hold none.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{KeyedFields, Lens, Mounted, RwSignal, Sink, flush, install};
///
/// /// One record.
/// #[derive(Clone)]
/// struct Record {
///     /// What it is called.
///     name: String,
///     /// How many there are.
///     qty: u32,
/// }
///
/// const NAME: Lens<Record, String> = Lens::new(0, |r| &r.name, |r| &mut r.name);
/// const QTY: Lens<Record, u32> = Lens::new(1, |r| &r.qty, |r| &mut r.qty);
///
/// install().unwrap();
/// let app = Mounted::new();
/// let (table, order) = app.with(|| {
///     let table = KeyedFields::from_values(
///         (0..100_000_u64).map(|key| (key, Record { name: format!("item {key}"), qty: 0 })),
///     );
///     (table, RwSignal::new((0..100_000_u64).collect::<Vec<_>>()))
/// });
/// assert_eq!(table.watched_fields(), 0);
///
/// // A row, as a view would build it for key 7: one binding per cell.
/// let row = Mounted::new();
/// let shown = std::rc::Rc::new(std::cell::Cell::new(0));
/// let cell = row.with(|| {
///     let shown = std::rc::Rc::clone(&shown);
///     Sink::new(move || shown.set(table.get(&7, QTY).unwrap_or_default()))
/// });
/// assert_eq!(table.watched_fields(), 1);
///
/// table.set(&7, QTY, 3);
/// flush();
/// assert_eq!(shown.get(), 3);
///
/// // The row goes out of view: its field is no longer watched.
/// drop(cell);
/// row.unmount();
/// assert_eq!(table.watched_fields(), 0);
/// # let _ = (order, NAME);
/// app.unmount();
/// ```
pub struct KeyedFields<K: 'static, T: 'static> {
    /// The shared state, freed with the owner.
    inner: StoredValue<Rc<Inner<K, T>>, LocalStorage>,
}

impl<K: 'static, T: 'static> Clone for KeyedFields<K, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: 'static, T: 'static> Copy for KeyedFields<K, T> {}

impl<K: 'static, T: 'static> fmt::Debug for KeyedFields<K, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyedFields").finish_non_exhaustive()
    }
}

/// The values and the signals of one collection.
struct Inner<K, T> {
    /// The values, by key.
    values: RefCell<HashMap<K, T>>,
    /// The signals of the fields that are read.
    signals: RefCell<Signals<K>>,
}

/// The signals of one collection, by key, and the bookkeeping that frees them.
struct Signals<K> {
    /// The signals of each key that has one.
    keys: HashMap<K, SmallVec<[FieldSignal; 1]>>,
    /// How many signals are held.
    held: usize,
    /// How many held signals have at least one owner.
    watched: usize,
    /// Signals that lost their last owner since the last sweep. An entry can be watched again,
    /// or listed twice, so a sweep checks each one.
    idle: Vec<(K, u8)>,
}

/// The signal of one field of one key.
struct FieldSignal {
    /// Which field, or [`WHOLE`].
    slot: u8,
    /// What readers subscribe to.
    trigger: ArcTrigger,
    /// The owners that read it and have a cleanup registered, by [`Owner::debug_id`].
    owners: SmallVec<[usize; 1]>,
}

impl<K, T> KeyedFields<K, T>
where
    K: Eq + Hash + Clone + 'static,
    T: 'static,
{
    /// Makes an empty collection, owned by the current owner.
    #[must_use]
    #[track_caller]
    pub fn new() -> Self {
        Self::from_values(std::iter::empty())
    }

    /// Makes a collection that holds `values`, owned by the current owner.
    ///
    /// No signal is made. A later value for a key replaces an earlier one.
    #[must_use]
    #[track_caller]
    pub fn from_values(values: impl IntoIterator<Item = (K, T)>) -> Self {
        crate::executor::assert_owner("KeyedFields::new");
        Self {
            inner: StoredValue::new_local(Rc::new(Inner {
                values: RefCell::new(values.into_iter().collect()),
                signals: RefCell::new(Signals {
                    keys: HashMap::new(),
                    held: 0,
                    watched: 0,
                    idle: Vec::new(),
                }),
            })),
        }
    }

    /// The field `lens` of `key`, cloned, subscribing the reader to that field.
    ///
    /// `None` when the collection holds no value for `key`. The reader is subscribed also then,
    /// so a later [`insert`](Self::insert) of `key` reaches it.
    pub fn get<V: Clone>(&self, key: &K, lens: Lens<T, V>) -> Option<V> {
        self.with(key, lens, V::clone)
    }

    /// Runs `f` with the field `lens` of `key`, subscribing the reader to that field.
    ///
    /// Do not write the collection from inside `f`.
    pub fn with<V, R>(&self, key: &K, lens: Lens<T, V>, f: impl FnOnce(&V) -> R) -> Option<R> {
        let inner = self.inner()?;
        self.track(&inner, key, lens.slot);
        let values = inner.values.borrow();
        values.get(key).map(|value| f((lens.read)(value)))
    }

    /// Runs `f` with the whole value of `key`, subscribing the reader to every field of it.
    ///
    /// Do not write the collection from inside `f`.
    pub fn with_value<R>(&self, key: &K, f: impl FnOnce(&T) -> R) -> Option<R> {
        let inner = self.inner()?;
        self.track(&inner, key, WHOLE);
        let values = inner.values.borrow();
        values.get(key).map(f)
    }

    /// Runs `f` with the whole value of `key`, and subscribes nothing.
    ///
    /// Do not write the collection from inside `f`.
    pub fn with_untracked<R>(&self, key: &K, f: impl FnOnce(&T) -> R) -> Option<R> {
        let inner = self.inner()?;
        let values = inner.values.borrow();
        values.get(key).map(f)
    }

    /// Writes the field `lens` of `key`, and notifies the readers of that field and of the whole
    /// value.
    ///
    /// Returns `false`, and writes nothing, when the collection holds no value for `key`.
    pub fn set<V>(&self, key: &K, lens: Lens<T, V>, value: V) -> bool {
        self.update(key, lens, |field| *field = value).is_some()
    }

    /// Changes the field `lens` of `key` in place, and notifies the readers of that field.
    ///
    /// `None`, with `f` not called, when the collection holds no value for `key`. Do not read or
    /// write the collection from inside `f`.
    pub fn update<V, R>(
        &self,
        key: &K,
        lens: Lens<T, V>,
        f: impl FnOnce(&mut V) -> R,
    ) -> Option<R> {
        let inner = self.inner()?;
        let result = {
            let mut values = inner.values.borrow_mut();
            f((lens.write)(values.get_mut(key)?))
        };
        notify(&inner, key, Some(lens.slot));
        Some(result)
    }

    /// Changes the whole value of `key` in place, and notifies every reader of it.
    ///
    /// `None`, with `f` not called, when the collection holds no value for `key`. Do not read or
    /// write the collection from inside `f`.
    pub fn update_value<R>(&self, key: &K, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let inner = self.inner()?;
        let result = {
            let mut values = inner.values.borrow_mut();
            f(values.get_mut(key)?)
        };
        notify(&inner, key, None);
        Some(result)
    }

    /// Puts `value` under `key`, notifies every reader of `key`, and returns the value it
    /// replaced.
    pub fn insert(&self, key: K, value: T) -> Option<T> {
        let inner = self.inner()?;
        let old = inner.values.borrow_mut().insert(key.clone(), value);
        notify(&inner, &key, None);
        old
    }

    /// Takes the value of `key` out, and notifies every reader of `key`.
    pub fn remove(&self, key: &K) -> Option<T> {
        let inner = self.inner()?;
        let old = inner.values.borrow_mut().remove(key);
        if old.is_some() {
            notify(&inner, key, None);
        }
        old
    }

    /// Whether the collection holds a value for `key`. Subscribes nothing.
    #[must_use]
    pub fn contains_key(&self, key: &K) -> bool {
        self.inner()
            .is_some_and(|inner| inner.values.borrow().contains_key(key))
    }

    /// How many values the collection holds. Subscribes nothing.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner().map_or(0, |inner| inner.values.borrow().len())
    }

    /// Whether the collection holds no value. Subscribes nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many field signals at least one live owner watches.
    ///
    /// Diagnostic. With the rows of a list as the readers, it follows the mounted rows, and it
    /// falls to zero when they unmount.
    #[must_use]
    pub fn watched_fields(&self) -> usize {
        self.inner()
            .map_or(0, |inner| inner.signals.borrow().watched)
    }

    /// How many field signals are held: the watched ones, and the ones that wait for a sweep.
    ///
    /// Diagnostic. Signals that lost their last owner are dropped in batches, when there are more
    /// of them than 64 or than the watched ones, whichever is more.
    #[must_use]
    pub fn held_fields(&self) -> usize {
        self.inner().map_or(0, |inner| inner.signals.borrow().held)
    }

    /// The shared state, while the owner lives.
    fn inner(&self) -> Option<Rc<Inner<K, T>>> {
        self.inner.try_read_value().map(|inner| Rc::clone(&inner))
    }

    /// Subscribes the current reader to `slot` of `key`, and registers its owner once.
    fn track(&self, inner: &Inner<K, T>, key: &K, slot: u8) {
        if Observer::get().is_none() {
            return;
        }
        let owner = Owner::current().map(|owner| owner.debug_id());
        let (trigger, register) = inner.signals.borrow_mut().watch(key, slot, owner);
        trigger.track();
        if let (true, Some(owner)) = (register, owner) {
            let this = *self;
            let key = key.clone();
            on_cleanup_local(move || {
                if let Some(inner) = this.inner() {
                    inner.signals.borrow_mut().release(key, slot, owner);
                }
            });
        }
    }
}

impl<K, T> Default for KeyedFields<K, T>
where
    K: Eq + Hash + Clone + 'static,
    T: 'static,
{
    #[track_caller]
    fn default() -> Self {
        Self::new()
    }
}

/// Notifies the readers of `slot` of `key` and the readers of its whole value, or every reader of
/// `key` when `slot` is `None`.
fn notify<K: Eq + Hash, T>(inner: &Inner<K, T>, key: &K, slot: Option<u8>) {
    let triggers: SmallVec<[ArcTrigger; 2]> = {
        let signals = inner.signals.borrow();
        let Some(fields) = signals.keys.get(key) else {
            return;
        };
        fields
            .iter()
            .filter(|field| slot.is_none_or(|slot| field.slot == slot || field.slot == WHOLE))
            .map(|field| field.trigger.clone())
            .collect()
    };
    for trigger in triggers {
        trigger.notify();
    }
}

impl<K: Eq + Hash + Clone> Signals<K> {
    /// The signal of `slot` of `key`, made if needed, and whether `owner` must register a cleanup.
    ///
    /// A signal made for a reader with no owner is listed as idle at once, so that a sweep can
    /// drop it.
    fn watch(&mut self, key: &K, slot: u8, owner: Option<usize>) -> (ArcTrigger, bool) {
        let fields = match self.keys.get_mut(key) {
            Some(fields) => fields,
            None => self.keys.entry(key.clone()).or_default(),
        };
        let mut made = false;
        let field = match fields.iter().position(|field| field.slot == slot) {
            Some(index) => &mut fields[index],
            None => {
                made = true;
                self.held += 1;
                fields.push(FieldSignal {
                    slot,
                    trigger: ArcTrigger::new(),
                    owners: SmallVec::new(),
                });
                fields.last_mut().expect("a field was pushed")
            }
        };
        let register = match owner {
            Some(owner) if !field.owners.contains(&owner) => {
                if field.owners.is_empty() {
                    self.watched += 1;
                }
                field.owners.push(owner);
                true
            }
            _ => false,
        };
        let trigger = field.trigger.clone();
        if made && owner.is_none() {
            self.idle.push((key.clone(), slot));
            self.sweep_if_due();
        }
        (trigger, register)
    }

    /// Removes `owner` from `slot` of `key`, and sweeps when enough signals are unwatched.
    fn release(&mut self, key: K, slot: u8, owner: usize) {
        let Some(field) = self
            .keys
            .get_mut(&key)
            .and_then(|fields| fields.iter_mut().find(|field| field.slot == slot))
        else {
            return;
        };
        let Some(index) = field.owners.iter().position(|&id| id == owner) else {
            return;
        };
        field.owners.swap_remove(index);
        if !field.owners.is_empty() {
            return;
        }
        // The signal stays until a sweep: an effect that runs again releases its fields just
        // before it reads them again, and keeps the same signal that way.
        self.watched -= 1;
        self.idle.push((key, slot));
        self.sweep_if_due();
    }

    /// Sweeps when more signals are idle than 64 or than the watched ones, whichever is more.
    fn sweep_if_due(&mut self) {
        if self.idle.len() > IDLE_MIN.max(self.watched) {
            self.sweep();
        }
    }

    /// Drops every listed signal that is still unwatched.
    fn sweep(&mut self) {
        for (key, slot) in std::mem::take(&mut self.idle) {
            let Some(fields) = self.keys.get_mut(&key) else {
                continue;
            };
            let before = fields.len();
            fields.retain(|field| field.slot != slot || !field.owners.is_empty());
            self.held -= before - fields.len();
            if fields.is_empty() {
                self.keys.remove(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::executor::{flush, install};
    use crate::own::{Mounted, Scope};
    use crate::reexport::RenderEffect;
    use crate::sink::Sink;

    /// A record with three fields.
    #[derive(Clone, Debug, PartialEq)]
    struct Record {
        /// Text.
        name: String,
        /// A count.
        qty: u32,
        /// A flag.
        tall: bool,
    }

    /// The name field.
    const NAME: Lens<Record, String> = Lens::new(0, |r| &r.name, |r| &mut r.name);
    /// The count field.
    const QTY: Lens<Record, u32> = Lens::new(1, |r| &r.qty, |r| &mut r.qty);
    /// The flag field.
    const TALL: Lens<Record, bool> = Lens::new(2, |r| &r.tall, |r| &mut r.tall);

    /// A record for `key`.
    fn record(key: u64) -> Record {
        Record {
            name: format!("r{key}"),
            qty: 0,
            tall: false,
        }
    }

    /// A collection of `n` records under a root scope.
    fn fixture(n: u64) -> (Mounted, KeyedFields<u64, Record>) {
        install().unwrap();
        std::mem::forget(crate::zone::enter_non_reactive_zone());
        let root = Mounted::new();
        let table = root.with(|| KeyedFields::from_values((0..n).map(|key| (key, record(key)))));
        (root, table)
    }

    /// Counts the runs of a sink that reads `lens` of `key`.
    fn counting_sink<V: Clone + 'static>(
        table: KeyedFields<u64, Record>,
        key: u64,
        lens: Lens<Record, V>,
        runs: &Rc<Cell<usize>>,
    ) -> Sink {
        let runs = Rc::clone(runs);
        Sink::new(move || {
            table.get(&key, lens);
            runs.set(runs.get() + 1);
        })
    }

    #[test]
    fn values_make_no_signal() {
        let (root, table) = fixture(10_000);
        assert_eq!(table.len(), 10_000);
        assert_eq!(table.get(&5, NAME).as_deref(), Some("r5"));
        assert!(table.set(&5, QTY, 4));
        assert_eq!(table.with_untracked(&5, |r| r.qty), Some(4));
        assert_eq!(
            table.held_fields(),
            0,
            "reads with no observer make nothing"
        );
        root.unmount();
    }

    #[test]
    fn a_write_re_runs_the_readers_of_that_field_only() {
        let (root, table) = fixture(4);
        let row = root.with(Scope::new).mount();
        let (name_runs, qty_runs) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let sinks = row.with(|| {
            [
                counting_sink(table, 1, NAME, &name_runs),
                counting_sink(table, 1, QTY, &qty_runs),
            ]
        });
        assert_eq!(table.watched_fields(), 2);

        table.set(&1, QTY, 9);
        flush();
        assert_eq!((name_runs.get(), qty_runs.get()), (1, 2));

        table.set(&2, QTY, 9);
        flush();
        assert_eq!((name_runs.get(), qty_runs.get()), (1, 2), "another key");

        table.insert(1, record(1));
        flush();
        assert_eq!(
            (name_runs.get(), qty_runs.get()),
            (2, 3),
            "insert reaches every field"
        );

        drop(sinks);
        row.unmount();
        root.unmount();
    }

    #[test]
    fn a_sink_that_re_runs_registers_its_owner_once() {
        let (root, table) = fixture(2);
        let row = root.with(Scope::new).mount();
        let runs = Rc::new(Cell::new(0));
        let sink = row.with(|| counting_sink(table, 0, QTY, &runs));
        for n in 1..=100 {
            table.set(&0, QTY, n);
            flush();
        }
        assert_eq!(runs.get(), 101);
        assert_eq!(table.held_fields(), 1);

        drop(sink);
        row.unmount();
        assert_eq!(table.watched_fields(), 0);
        root.unmount();
    }

    #[test]
    fn an_effect_that_re_runs_keeps_its_signal() {
        let (root, table) = fixture(2);
        let row = root.with(Scope::new).mount();
        let seen = Rc::new(Cell::new(0));
        let effect = row.with(|| {
            let seen = Rc::clone(&seen);
            RenderEffect::new(move |_| seen.set(table.get(&1, QTY).unwrap_or_default()))
        });
        for n in 1..=200 {
            table.set(&1, QTY, n);
            flush();
            assert_eq!(seen.get(), n);
        }
        assert_eq!(table.watched_fields(), 1);
        assert_eq!(table.held_fields(), 1);

        drop(effect);
        row.unmount();
        root.unmount();
    }

    #[test]
    fn a_memo_between_the_table_and_a_sink_keeps_its_signal() {
        let (root, table) = fixture(2);
        let row = root.with(Scope::new).mount();
        let seen = Rc::new(Cell::new(0));
        let sink = row.with(|| {
            let doubled =
                crate::reexport::Memo::new(move |_| table.get(&0, QTY).unwrap_or_default() * 2);
            let seen = Rc::clone(&seen);
            Sink::new(move || seen.set(reactive_graph::traits::Get::get(&doubled)))
        });
        for n in 1..=100 {
            table.set(&0, QTY, n);
            flush();
            assert_eq!(seen.get(), n * 2);
        }
        assert_eq!(table.held_fields(), 1);

        drop(sink);
        row.unmount();
        assert_eq!(table.watched_fields(), 0);
        root.unmount();
    }

    #[test]
    fn the_handle_crosses_threads() {
        fn shareable<T: Send + Sync + Copy>() {}
        shareable::<KeyedFields<u64, Rc<Record>>>();
    }

    #[test]
    fn whole_value_readers_see_every_field() {
        let (root, table) = fixture(2);
        let row = root.with(Scope::new).mount();
        let runs = Rc::new(Cell::new(0));
        let sink = row.with(|| {
            let runs = Rc::clone(&runs);
            Sink::new(move || {
                table.with_value(&0, |r| r.tall);
                runs.set(runs.get() + 1);
            })
        });
        table.set(&0, TALL, true);
        flush();
        table.update(&0, NAME, |name| name.push('!'));
        flush();
        assert_eq!(runs.get(), 3);

        drop(sink);
        row.unmount();
        root.unmount();
    }

    #[test]
    fn a_missing_key_is_watched_until_it_arrives() {
        let (root, table) = fixture(0);
        let row = root.with(Scope::new).mount();
        let seen = Rc::new(RefCell::new(None));
        let sink = row.with(|| {
            let seen = Rc::clone(&seen);
            Sink::new(move || *seen.borrow_mut() = table.get(&3, NAME))
        });
        assert_eq!(*seen.borrow(), None);
        assert!(!table.set(&3, NAME, "x".to_owned()));

        table.insert(3, record(3));
        flush();
        assert_eq!(seen.borrow().as_deref(), Some("r3"));

        table.remove(&3);
        flush();
        assert_eq!(*seen.borrow(), None);

        drop(sink);
        row.unmount();
        root.unmount();
    }

    #[test]
    fn scrolling_through_many_rows_keeps_the_signals_bounded() {
        let (root, table) = fixture(100_000);
        let rows = root.with(Scope::new);
        let runs = Rc::new(Cell::new(0));
        // A window of 20 rows, each reading three fields, moved one row at a time.
        let mut window = std::collections::VecDeque::new();
        let mut peak = 0;
        for first in 0..5_000_u64 {
            while window.len() < 20 {
                let key = first + window.len() as u64;
                let row = rows.mount();
                let sinks = row.with(|| {
                    [
                        counting_sink(table, key, NAME, &runs),
                        counting_sink(table, key, QTY, &runs),
                        counting_sink(table, key, TALL, &runs),
                    ]
                });
                window.push_back((row, sinks));
            }
            assert_eq!(table.watched_fields(), 60);
            peak = peak.max(table.held_fields());
            let (row, sinks) = window.pop_front().expect("a full window");
            drop(sinks);
            row.unmount();
        }
        assert!(peak <= 60 + IDLE_MIN + 1, "held {peak} signals at most");

        for (row, sinks) in window.drain(..) {
            drop(sinks);
            row.unmount();
        }
        assert_eq!(table.watched_fields(), 0);
        assert!(table.held_fields() <= IDLE_MIN + 1);
        root.unmount();
    }

    #[test]
    fn a_reader_with_no_owner_keeps_no_signal() {
        let (root, table) = fixture(1_000);
        let tick = root.with(|| crate::RwSignal::new(0_u64));
        let row = root.with(Scope::new).mount();
        let runs = Rc::new(Cell::new(0));
        let sink = row.with(|| {
            let runs = Rc::clone(&runs);
            Sink::new(move || {
                let key = reactive_graph::traits::Get::get(&tick);
                table.get(&key, QTY);
                runs.set(runs.get() + 1);
            })
        });
        assert_eq!(table.watched_fields(), 1);

        // The sink outlives its owner, and runs again with no owner on each tick.
        row.unmount();
        for n in 1..=500 {
            reactive_graph::traits::Set::set(&tick, n);
            flush();
        }
        assert_eq!(runs.get(), 501);
        assert_eq!(table.watched_fields(), 0);
        assert!(
            table.held_fields() <= IDLE_MIN + 1,
            "held {} signals",
            table.held_fields()
        );

        drop(sink);
        root.unmount();
    }

    #[test]
    fn two_rows_on_one_key_share_its_signal() {
        let (root, table) = fixture(2);
        let rows = root.with(Scope::new);
        let runs = Rc::new(Cell::new(0));
        let (first, second) = (rows.mount(), rows.mount());
        let a = first.with(|| counting_sink(table, 0, QTY, &runs));
        let b = second.with(|| counting_sink(table, 0, QTY, &runs));
        assert_eq!(table.held_fields(), 1);

        drop(a);
        first.unmount();
        assert_eq!(table.watched_fields(), 1);
        table.set(&0, QTY, 1);
        flush();
        assert_eq!(runs.get(), 3, "the second row still hears the write");

        drop(b);
        second.unmount();
        assert_eq!(table.watched_fields(), 0);
        root.unmount();
    }

    #[test]
    fn a_disposed_collection_reads_as_empty() {
        let (root, table) = fixture(3);
        root.unmount();
        assert_eq!(table.get(&1, NAME), None);
        assert!(!table.set(&1, QTY, 1));
        assert_eq!(table.len(), 0);
    }
}
