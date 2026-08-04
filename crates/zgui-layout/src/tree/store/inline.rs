//! What an inline formatting context resolved to, and the flattened form it is holding.

use zgui_dom::side::BoxKey;

use crate::inline::content::memo::Flattened;
use crate::inline::resolved::InlineResolution;
use crate::tree::store::LayoutStore;

impl LayoutStore {
    /// The lines one inline formatting context resolved to.
    pub fn inline_resolution(&self, key: BoxKey) -> Option<&InlineResolution> {
        self.layout.get(key)?.inline.as_deref()
    }

    /// Records what one inline formatting context resolved to.
    pub fn set_inline_resolution(&mut self, key: BoxKey, resolution: InlineResolution) {
        if self.layout.get(key).is_none() {
            return;
        }
        let previous = self
            .layout
            .get(key)
            .and_then(|state| state.inline.as_ref())
            .map(|held| held.paragraph);
        let next = resolution.paragraph;
        if previous != Some(next) {
            if let Some(previous) = previous {
                self.release_paragraph(previous);
            }
            self.retain_paragraph(next);
        }
        if let Some(state) = self.layout.get_mut(key) {
            state.inline = Some(Box::new(resolution));
        }
    }

    /// Removes one inline resolution and releases the paragraph identifier it held.
    pub(crate) fn take_inline_resolution(&mut self, key: BoxKey) -> Option<Box<InlineResolution>> {
        let resolution = self.layout.get_mut(key)?.inline.take();
        if let Some(held) = &resolution {
            self.release_paragraph(held.paragraph);
        }
        resolution
    }

    /// How many times this store has flattened an inline formatting context into the string a
    /// shaper is handed.
    ///
    /// Flattening walks every character of a paragraph, so this is the number that separates a
    /// document whose paragraphs are measured many times over from one whose paragraphs are rebuilt
    /// as often as they are measured. It counts flattenings performed, never contexts that exist:
    /// a store that has laid the same paragraph out at twenty widths reports one.
    pub fn flattenings(&self) -> u64 {
        self.flattenings
    }

    /// The flattened form one inline formatting context is holding, if it is holding one.
    pub(crate) fn flattened(&self, key: BoxKey) -> Option<&Flattened> {
        self.layout.get(key)?.flattened.as_deref()
    }

    /// Holds one inline formatting context's flattened form, replacing whatever it held.
    pub(crate) fn hold_flattened(&mut self, key: BoxKey, flattened: Flattened) {
        self.flattenings += 1;
        if let Some(state) = self.layout.get_mut(key) {
            state.flattened = Some(Box::new(flattened));
        }
    }

    /// Drops the flattened form one box is holding, and reports whether it was holding one.
    ///
    /// The held form is checked against the sequence of boxes it was flattened from, so it survives
    /// anything that leaves that sequence alone — including a box whose characters were rewritten
    /// where it stands, which is the same box in the same place. That rewrite is the one change
    /// the check cannot see, and this is how it is told.
    pub(crate) fn forget_flattened(&mut self, key: BoxKey) -> bool {
        self.layout
            .get_mut(key)
            .is_some_and(|state| state.flattened.take().is_some())
    }
}
