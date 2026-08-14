//! What the parts of a page of settings read to find each other.

use std::collections::BTreeMap;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, UnsyncCallback};
use zgui_ui_primitives::{Binding, Controllable};

/// What an entry in the page list and a pane read to know whether they are the ones showing.
#[derive(Copy, Clone)]
pub struct SettingsContext {
    /// Which page is showing.
    page: Controllable<String>,
    /// One handle per entry, by page, so a pane can name the entry that labels it.
    entries: RwSignal<BTreeMap<String, NodeRef>, LocalStorage>,
    /// One handle per pane, by page, so an entry can name what it controls.
    panes: RwSignal<BTreeMap<String, NodeRef>, LocalStorage>,
}

impl SettingsContext {
    /// Wires the page choice up from the root's three props.
    pub(crate) fn new(
        page: Binding<String>,
        default_page: String,
        on_page_change: Option<UnsyncCallback<String>>,
    ) -> Self {
        Self {
            page: Controllable::new(page, default_page, on_page_change),
            entries: RwSignal::new_local(BTreeMap::new()),
            panes: RwSignal::new_local(BTreeMap::new()),
        }
    }

    /// The settings the calling scope is inside, when it is inside any.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Which page is showing.
    #[must_use]
    pub fn page(self) -> String {
        self.page.get()
    }

    /// Whether the page called `value` is the one showing.
    #[must_use]
    pub fn is_selected(self, value: &str) -> bool {
        self.page.get() == value
    }

    /// Shows the page called `value`.
    pub fn select(self, value: &str) {
        self.page.set(value.to_owned());
    }

    /// The handle the entry for `value` binds, minting one the first time it is asked for.
    ///
    /// Held by the root rather than by either part, because the two parts need *each other's*
    /// element and are built in either order — and a pane that is not mounted still has to be
    /// nameable by the entry that would show it.
    #[must_use]
    pub fn entry_of(self, value: &str) -> NodeRef {
        Self::handle(self.entries, value)
    }

    /// The handle the pane for `value` binds.
    #[must_use]
    pub fn pane_of(self, value: &str) -> NodeRef {
        Self::handle(self.panes, value)
    }

    /// One map's entry for `value`, minted on first use.
    fn handle(map: RwSignal<BTreeMap<String, NodeRef>, LocalStorage>, value: &str) -> NodeRef {
        if let Some(found) = map.with_untracked(|map| map.get(value).copied()) {
            return found;
        }
        let node = NodeRef::new();
        map.update(|map| {
            map.insert(value.to_owned(), node);
        });
        node
    }
}

/// Where a [`SettingsGroup`](crate::SettingsGroup) records the elements that name and describe it.
///
/// The group is a group to a reader, and a group with no name is one a reader meets as an
/// anonymous box. Its heading knows the words and the group knows the relation, so the group hands
/// the handles down and the heading binds them.
#[derive(Copy, Clone)]
pub(crate) struct SettingsGroupContext {
    /// The heading over the group.
    pub(crate) label: NodeRef,
    /// The line under the heading qualifying it.
    pub(crate) description: NodeRef,
}

impl SettingsGroupContext {
    /// The group the calling scope is inside, when it is inside one.
    pub(crate) fn current() -> Option<Self> {
        use_local_context::<Self>()
    }
}
