//! The options a listbox holds, and which of them is being walked.

use std::collections::BTreeMap;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, UnsyncCallback};
use zgui_ui_primitives::{Binding, Collection, Controllable, ItemId};

use crate::listbox::catalogue::ListboxCatalogue;
use crate::listbox::keys::{ListboxAction, action_for};
use crate::listbox::labels::ListboxLabels;
use crate::listbox::option::{ListboxEntry, ListboxOption};
use crate::overlay::OverlayState;

/// A set of options, the one being walked, and the one that is chosen.
///
/// The whole of what a select, a combobox and a command palette have in common. Three things it
/// keeps that nothing else can:
///
/// * **the options, in tree order** — announced by the options themselves, because a parent cannot
///   enumerate children that are behind a conditional or inside a list;
/// * **which one is active** — the one the arrow keys are on, which is *not* the one that has
///   focus: focus stays on the trigger or the text field, and the active option is named to a
///   reader by `active_descendant`;
/// * **which one is chosen** — controlled or not, exactly as everywhere else in this library.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui::listbox::Listbox;
/// use zgui_ui::overlay::OverlayState;
/// use zgui_ui_primitives::Binding;
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     let surface = OverlayState::uncontrolled(false, None);
///     let listbox = Listbox::new(surface, Binding::Unbound, None, None).provide();
///     assert!(listbox.chosen().is_empty(), "nothing is chosen yet");
///     assert!(listbox.active().is_none(), "and nothing is being walked");
///     assert!(Listbox::current().is_some());
/// });
/// scope.unmount();
/// ```
#[derive(Copy, Clone)]
pub struct Listbox {
    /// The options, in the order a reader meets them.
    collection: Collection,
    /// What is known about each of them.
    options: RwSignal<BTreeMap<ItemId, ListboxOption>, LocalStorage>,
    /// Which one the arrow keys are on.
    active: RwSignal<Option<ItemId>, LocalStorage>,
    /// Which one is chosen.
    value: Controllable<String>,
    /// What has been typed to narrow the list, for the controls that have a field.
    filter: RwSignal<String, LocalStorage>,
    /// What each value reads as, learned from the options that say so.
    ///
    /// A closed select has no options mounted: they are its list, and its list is gone. Without
    /// this the control would show its placeholder over a value it already has, from the moment it
    /// is built until the list has been opened once — which is the whole of the time anybody looks
    /// at it.
    labels: ListboxLabels,
    /// The surface the options are on.
    surface: OverlayState,
    /// Whether choosing something closes that surface.
    dismisses: bool,
}

impl Listbox {
    /// Wires a listbox up to an overlay and to a value's three props.
    #[must_use]
    pub fn new(
        surface: OverlayState,
        value: Binding<String>,
        default_value: Option<String>,
        on_change: Option<UnsyncCallback<String>>,
    ) -> Self {
        Self {
            collection: Collection::new(),
            options: RwSignal::new_local(BTreeMap::new()),
            active: RwSignal::new_local(None),
            value: Controllable::new(value, default_value.unwrap_or_default(), on_change),
            filter: RwSignal::new_local(String::new()),
            labels: ListboxLabels::new(),
            surface,
            dismisses: true,
        }
    }

    /// The same, for a list that is always on the surface it is written on.
    ///
    /// A command palette is one: its list is not a popup, so choosing something must not try to
    /// close anything — and a list that closed itself would answer the next arrow key by opening
    /// again instead of moving.
    #[must_use]
    pub const fn inline(mut self) -> Self {
        self.dismisses = false;
        self
    }

    /// Publishes this, and the collection its options register with, to every scope below.
    pub fn provide(self) -> Self {
        provide_local_context(self);
        provide_local_context(self.collection);
        self
    }

    /// The listbox the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// The surface the options are on.
    #[must_use]
    pub fn surface(&self) -> OverlayState {
        self.surface
    }

    /// Learns what one option's value reads as, without putting it on the list.
    ///
    /// What an option built inside a [`ListboxCatalogueOf`](crate::listbox::ListboxCatalogueOf)
    /// does instead of registering: it is not on the screen, so it is not something to walk to or
    /// to choose, and the only thing wanted from it is the one thing only it can say.
    pub fn describe(&self, option: ListboxOption) {
        self.labels.learn(option);
    }

    /// Registers one option, and takes it out again when the calling scope goes away.
    pub fn register(&self, option: ListboxOption) -> ItemId {
        let id = self.collection.register(option.node());
        self.options.update(|options| {
            options.insert(id, option);
        });
        let listbox = *self;
        on_cleanup_local(move || {
            listbox.options.try_update(|options| {
                options.remove(&id);
            });
        });
        id
    }

    /// Every option, in tree order.
    #[must_use]
    pub fn entries(&self) -> Vec<ListboxEntry> {
        let known = self.options.get();
        self.collection
            .items()
            .into_iter()
            .filter_map(|item| {
                known
                    .get(&item.id())
                    .map(|option| ListboxEntry::new(item.id(), option.clone()))
            })
            .collect()
    }

    /// The same, without subscribing to the set.
    ///
    /// What the keyboard model reads: a key handler runs outside any reactive scope, and a read
    /// that tried to subscribe there would subscribe nothing to anything.
    #[must_use]
    pub fn entries_untracked(&self) -> Vec<ListboxEntry> {
        let known = self.options.get_untracked();
        self.collection
            .items_untracked()
            .into_iter()
            .filter_map(|item| {
                known
                    .get(&item.id())
                    .map(|option| ListboxEntry::new(item.id(), option.clone()))
            })
            .collect()
    }

    /// Every option that can actually be chosen, in tree order, without subscribing.
    #[must_use]
    pub fn choosable(&self) -> Vec<ListboxEntry> {
        self.entries_untracked()
            .into_iter()
            .filter(|entry| !entry.option().is_disabled())
            .collect()
    }

    /// Which option the arrow keys are on.
    ///
    /// Until something has been walked or chosen, that is the **first** option that can be
    /// chosen — so an open list always has somewhere for `active_descendant` to point and for
    /// <kbd>Enter</kbd> to land, rather than a first arrow key that appears to do nothing.
    #[must_use]
    pub fn active(&self) -> Option<ItemId> {
        match self.active.get() {
            Some(id) => Some(id),
            None => self
                .entries()
                .into_iter()
                .find(|entry| !entry.option().is_disabled())
                .map(|entry| entry.id()),
        }
    }

    /// The same, without subscribing.
    #[must_use]
    fn active_untracked(&self) -> Option<ItemId> {
        match self.active.get_untracked() {
            Some(id) => Some(id),
            None => self.choosable().first().map(ListboxEntry::id),
        }
    }

    /// The element of the option the arrow keys are on, which is what a reader is pointed at.
    #[must_use]
    pub fn active_node(&self) -> NodeRef {
        let active = self.active();
        self.entries()
            .into_iter()
            .find(|entry| Some(entry.id()) == active)
            .map_or_else(NodeRef::new, |entry| entry.option().node())
    }

    /// Makes `id` the one the arrow keys are on.
    pub fn set_active(&self, id: Option<ItemId>) {
        if self.active.get_untracked() != id {
            self.active.set(id);
        }
    }

    /// Which value is chosen.
    #[must_use]
    pub fn chosen(&self) -> String {
        self.value.get()
    }

    /// Whether `value` is the chosen one.
    #[must_use]
    pub fn is_chosen(&self, value: &str) -> bool {
        self.value.get() == value
    }

    /// What the chosen option reads as, for a trigger to show.
    ///
    /// From the option itself while the list is mounted, and from what the value has been said to
    /// read as otherwise — because a closed select has no options at all, and a control that
    /// answered `None` there would show its placeholder over a value the user has already chosen.
    #[must_use]
    pub fn chosen_text(&self) -> Option<String> {
        let chosen = self.value.get();
        if chosen.is_empty() {
            return None;
        }
        self.entries()
            .into_iter()
            .find(|entry| entry.option().value() == chosen)
            .map(|entry| entry.option().text())
            .filter(|text| !text.is_empty())
            .or_else(|| self.labels.of(&chosen))
    }

    /// Chooses `value`, and tells whoever asked to be told.
    pub fn choose(&self, value: &str) {
        self.value.set(value.to_owned());
    }

    /// Chooses one option: reports its value, runs whatever it does, and closes the list.
    ///
    /// The one path everything takes — a press on an option, <kbd>Enter</kbd> on the control —
    /// so a palette cannot end up running a command on one of the two and not on the other.
    pub fn take(&self, id: ItemId) {
        let Some(entry) = self
            .entries_untracked()
            .into_iter()
            .find(|entry| entry.id() == id)
        else {
            return;
        };
        if entry.option().is_disabled() {
            return;
        }
        self.choose(entry.option().value());
        entry.option().select();
        if self.dismisses {
            self.surface.close();
        }
    }

    /// What has been typed to narrow the list.
    #[must_use]
    pub fn filter(&self) -> String {
        self.filter.get()
    }

    /// Narrows the list, and puts the highlight back on the first option that survives.
    ///
    /// Both, because they are one act: a highlight left on an option the filter has just taken
    /// away is a highlight on nothing, and Enter would choose something the user cannot see.
    pub fn set_filter(&self, text: impl Into<String>) {
        self.filter.set(text.into());
        self.active.set(None);
    }

    /// Whether an option reading as `text` survives the filter.
    ///
    /// A substring rather than a prefix, and deliberately: a command palette is searched by any
    /// word of a command, not only by how it starts.
    #[must_use]
    pub fn matches(&self, text: &str) -> bool {
        let filter = self.filter.get();
        filter.is_empty() || text.to_lowercase().contains(&filter.to_lowercase())
    }

    /// Moves the highlight `steps` places among the options that can be chosen, wrapping.
    ///
    /// From nothing, a forward step lands on the first and a backward one on the last, which is
    /// what makes the down arrow open a select onto its first option.
    pub fn step(&self, steps: isize) {
        let entries = self.choosable();
        if entries.is_empty() {
            return;
        }
        let length = entries.len() as isize;
        let from = self
            .active_untracked()
            .and_then(|id| entries.iter().position(|entry| entry.id() == id));
        let at = match from {
            Some(at) => at as isize + steps,
            None if steps >= 0 => 0,
            None => length - 1,
        };
        let index = at.rem_euclid(length) as usize;
        self.set_active(Some(entries[index].id()));
    }

    /// Moves the highlight to the last option that can be chosen, or the first.
    pub fn to_end(&self, last: bool) {
        let entries = self.choosable();
        let target = if last {
            entries.last()
        } else {
            entries.first()
        };
        if let Some(entry) = target {
            self.set_active(Some(entry.id()));
        }
    }

    /// Puts the highlight on the chosen option, or on the first choosable one.
    ///
    /// What opening the list does: a select opened on *Amount* highlights *Amount*, so one press
    /// of the down arrow moves to the next one rather than back to the top.
    pub fn highlight_chosen(&self) {
        let entries = self.choosable();
        let chosen = self.value.get_untracked();
        let target = entries
            .iter()
            .find(|entry| entry.option().value() == chosen)
            .or_else(|| entries.first());
        self.set_active(target.map(ListboxEntry::id));
    }

    /// Carries out what a key press means, and reports whether anything came of it.
    ///
    /// The one place the whole keyboard model lives, so a select, a combobox and a palette cannot
    /// drift apart on what the down arrow does.
    pub fn handle(&self, key: &zgui::vocab::Key) -> bool {
        let open = self.surface.is_open_untracked();
        let Some(action) = action_for(key, open) else {
            return false;
        };
        match action {
            ListboxAction::Step(steps) => {
                if open {
                    self.step(steps);
                } else {
                    self.surface.open();
                    self.highlight_chosen();
                }
            }
            ListboxAction::End(last) => self.to_end(last),
            ListboxAction::Choose => {
                let Some(active) = self.active_untracked() else {
                    return false;
                };
                self.take(active);
            }
            ListboxAction::Close => {
                if !self.dismisses {
                    return false;
                }
                self.surface.close();
            }
        }
        true
    }
}

/// Registers `node` as an option of the enclosing [`Listbox`], when there is one.
///
/// `None` outside one, which is an ordinary answer: the same option component still renders and
/// still says what it is, and there is simply nothing for it to be an option of. `None` too inside
/// a [`ListboxCatalogueOf`](crate::listbox::ListboxCatalogueOf), where the option is built to be
/// read rather than to be chosen — so an option written once cannot end up on the list twice.
pub fn use_listbox_option(option: ListboxOption) -> Option<(Listbox, ItemId)> {
    let listbox = Listbox::current()?;
    if ListboxCatalogue::is_current() {
        listbox.describe(option);
        return None;
    }
    let id = listbox.register(option);
    Some((listbox, id))
}
