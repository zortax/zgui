//! Which axes of a box reserve a scrollbar gutter, and what holds one reserved.

use zgui_dom::side::BoxKey;

use crate::tree::store::LayoutStore;

impl LayoutStore {
    /// Which axes of one box were decided to scroll.
    pub fn auto_scroll(&self, key: BoxKey) -> (bool, bool) {
        self.layout
            .get(key)
            .and_then(Option::as_ref)
            .map_or((false, false), |state| state.auto_scroll)
    }

    /// The gutter one box keeps reserved while it is locked, if it is locked.
    pub fn scroll_lock(&self, key: BoxKey) -> Option<(bool, bool)> {
        self.layout
            .get(key)
            .and_then(Option::as_ref)
            .and_then(|state| state.scroll_lock)
    }

    /// Records which axes of one box were decided to scroll.
    pub(crate) fn set_auto_scroll(&mut self, key: BoxKey, axes: (bool, bool)) {
        if let Some(state) = self.layout.get_mut(key).as_mut() {
            state.auto_scroll = axes;
        }
    }

    /// Records, or clears, one box's held gutter.
    pub(crate) fn set_scroll_lock(&mut self, key: BoxKey, axes: Option<(bool, bool)>) {
        if let Some(state) = self.layout.get_mut(key).as_mut() {
            state.scroll_lock = axes;
        }
    }

    /// Which axes of one box reserve a scrollbar gutter whatever its own style says.
    ///
    /// The union of the decision an `overflow: auto` box reached and the gutter a locked container
    /// is holding, which is the whole of what layout itself contributes to the question.
    pub(crate) fn reserved_gutter(&self, key: BoxKey) -> (bool, bool) {
        let held = self.auto_scroll(key);
        match self.scroll_lock(key) {
            Some(locked) => (held.0 || locked.0, held.1 || locked.1),
            None => held,
        }
    }
}
