//! What each value reads as, kept where the option that says so is not.
//!
//! A closed select has no options: they are its list, and its list is not mounted. The text a value
//! reads as is nonetheless the option's own — written once, where the option is written — so a
//! control that asked the mounted options what its value reads as would answer with its placeholder
//! over a choice the user has already made, and would go on doing so until the list had been opened
//! once. That is a control which is wrong exactly while it is in the state it spends its life in.
//!
//! So the text is learned from the options and kept here, beside the value rather than beside the
//! element, and it outlives the mounting that taught it.

use std::collections::BTreeMap;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, RwSignal, StoredValue};

use crate::listbox::option::ListboxOption;

/// What each value reads as, for whoever has to show a choice without the option that made it.
///
/// `Copy`, so a control stores one without cloning.
#[derive(Copy, Clone)]
pub struct ListboxLabels {
    /// One entry per value that has ever described itself.
    known: RwSignal<BTreeMap<String, String>, LocalStorage>,
}

impl Default for ListboxLabels {
    fn default() -> Self {
        Self::new()
    }
}

impl ListboxLabels {
    /// An empty dictionary.
    #[must_use]
    pub fn new() -> Self {
        Self {
            known: RwSignal::new_local(BTreeMap::new()),
        }
    }

    /// Learns what `option` reads as, and keeps it after the option has gone.
    ///
    /// Held rather than read once: the text an option reads as is the text it *renders*, which
    /// nothing can answer until the element exists — so this waits for the handle to bind and
    /// writes the answer down then.
    ///
    /// Kept after the option is unmounted, deliberately. Forgetting on unmount would make this
    /// exactly as empty as the option list it stands in for, which is the whole of what it is for.
    pub fn learn(&self, option: ListboxOption) {
        let known = self.known;
        let watching = RenderEffect::new(move |_| {
            if option.node().get().is_none() {
                return;
            }
            let text = option.text();
            if text.is_empty() {
                return;
            }
            let value = option.value().to_owned();
            if known.with_untracked(|known| known.get(&value) == Some(&text)) {
                return;
            }
            known.update(|known| {
                known.insert(value, text);
            });
        });
        // Stored rather than dropped: an effect runs only for as long as something holds it, and
        // one dropped here would run once, before the element it is waiting for exists, and never
        // again.
        StoredValue::new_local(watching);
    }

    /// What `value` reads as, when anything has ever said.
    #[must_use]
    pub fn of(&self, value: &str) -> Option<String> {
        self.known.with(|known| known.get(value).cloned())
    }
}
