//! What an inline formatting context resolved to, and the flattened form it is holding.

use zgui_dom::side::BoxKey;

use crate::inline::content::memo::Flattened;
use crate::inline::resolved::InlineResolution;
use crate::tree::store::LayoutStore;

impl LayoutStore {
    /// The lines one inline formatting context resolved to.
    pub fn inline_resolution(&self, key: BoxKey) -> Option<&InlineResolution> {
        self.layout.get(key)?.as_ref()?.inline.as_deref()
    }

    /// Records what one inline formatting context resolved to.
    pub fn set_inline_resolution(&mut self, key: BoxKey, resolution: InlineResolution) {
        if self.layout.get(key).and_then(Option::as_ref).is_none() {
            return;
        }
        // The marks `text-overflow` cuts the lines with are paragraphs of their own, and they are
        // retained beside the context's: a mark whose shaping is evicted while a line still names it
        // is a line drawn with no ellipsis and no way to notice.
        let held = self
            .layout
            .get(key)
            .and_then(Option::as_ref)
            .and_then(|state| state.inline.as_ref());
        let previous: Vec<_> = held
            .map(|held| held.paragraphs().collect())
            .unwrap_or_default();
        let next: Vec<_> = resolution.paragraphs().collect();
        for id in &next {
            if !previous.contains(id) {
                self.retain_paragraph(*id);
            }
        }
        for id in &previous {
            if !next.contains(id) {
                self.release_paragraph(*id);
            }
        }
        if let Some(state) = self.layout.get_mut(key).as_mut() {
            state.inline = Some(Box::new(resolution));
        }
    }

    /// Removes one inline resolution and releases the paragraph identifiers it held.
    pub(crate) fn take_inline_resolution(&mut self, key: BoxKey) -> Option<Box<InlineResolution>> {
        let resolution = self.layout.get_mut(key).as_mut()?.inline.take();
        if let Some(held) = &resolution {
            for id in held.paragraphs().collect::<Vec<_>>() {
                self.release_paragraph(id);
            }
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
        self.layout.get(key)?.as_ref()?.flattened.as_deref()
    }

    /// The string a shaper was handed for one inline formatting context.
    ///
    /// The document's own text is not it: white space has been collapsed, tabs expanded and
    /// `text-transform` applied, and every offset the lines, the clusters and the hit answers are
    /// expressed in is an offset into *this*. Exposed because it is the only place the result of
    /// those three is visible — a case transform moves no edge, so an inspector or a parity harness
    /// that reads the fragment tree alone cannot see that it happened at all.
    pub fn generated_text(&self, key: BoxKey) -> Option<&str> {
        self.flattened(key).map(|flattened| flattened.text())
    }

    /// Holds one inline formatting context's flattened form, replacing whatever it held.
    pub(crate) fn hold_flattened(&mut self, key: BoxKey, flattened: Flattened) {
        self.flattenings += 1;
        if let Some(state) = self.layout.get_mut(key).as_mut() {
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
            .as_mut()
            .is_some_and(|state| state.flattened.take().is_some())
    }
}
