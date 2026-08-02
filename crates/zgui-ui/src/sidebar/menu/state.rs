//! What one entry's parts tell each other.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};

use crate::sidebar::shape::SidebarMenuSize;

/// What the parts of one entry tell each other.
///
/// An action or a count is placed against the entry rather than laid out in it, so where it goes
/// depends on how tall the entry's control is — and the control is its sibling, not its parent.
/// The entry itself is what they have in common, so it is where the answer is kept.
#[derive(Copy, Clone)]
pub struct SidebarMenuItemState {
    /// How tall the entry's control is.
    size: RwSignal<SidebarMenuSize, LocalStorage>,
    /// Whether the entry is the place being shown.
    active: RwSignal<bool, LocalStorage>,
    /// Whether anything is placed against the entry's right edge.
    crowded: RwSignal<bool, LocalStorage>,
}

impl Default for SidebarMenuItemState {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarMenuItemState {
    /// A state for an entry that has told nothing yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            size: RwSignal::new_local(SidebarMenuSize::Default),
            active: RwSignal::new_local(false),
            crowded: RwSignal::new_local(false),
        }
    }

    /// The entry the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// How tall the entry's control is.
    #[must_use]
    pub fn size(self) -> SidebarMenuSize {
        self.size.get()
    }

    /// Whether the entry is the place being shown.
    #[must_use]
    pub fn is_active(self) -> bool {
        self.active.get()
    }

    /// Whether anything is placed against the entry's right edge.
    #[must_use]
    pub fn is_crowded(self) -> bool {
        self.crowded.get()
    }

    /// Says how tall the control is, which the control does and nothing else should.
    pub(crate) fn take_size(self, size: SidebarMenuSize) {
        if self.size.get_untracked() != size {
            self.size.set(size);
        }
    }

    /// Says whether the entry is current, which the control does and nothing else should.
    pub(crate) fn take_active(self, active: bool) {
        if self.active.get_untracked() != active {
            self.active.set(active);
        }
    }

    /// Says that something now stands against the entry's right edge.
    pub(crate) fn crowd(self) {
        if !self.crowded.get_untracked() {
            self.crowded.set(true);
        }
    }
}
