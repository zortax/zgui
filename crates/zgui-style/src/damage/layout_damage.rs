//! Which half of text layout a style change actually costs.
//!
//! The hook the engine calls for embedder damage is an associated function with no receiver and no
//! context: it is handed two styles and nothing else, so the only classification it can make is
//! the conservative one — *any* layout-affecting change re-shapes. Shaping is the expensive half,
//! and a width change or an alignment change moves no glyph, so paying a shape for one is paying
//! about twenty-eight times what the change costs.
//!
//! What is missing at the hook is the ability to remember. Here there is one: the keys of each
//! element's last text style are kept, and the new style's keys are compared against them. A
//! change that leaves the shaping key alone cannot need a fresh shape, because the key is hashed
//! from exactly the properties the shaper reads — the classification and the hash are derived from
//! one definition, so a property cannot be classified one way and hashed the other.
//!
//! The narrowing is only ever applied *inside* a relayout: an element the engine did not give
//! layout damage to is not being re-shaped or re-broken by this at all.

use rustc_hash::FxHashMap;
use zgui_css::ComputedStyle;
use zgui_dom::{DocumentStore, NodeFlags, NodeKey};
use zgui_text_style::lower::{paragraph_style, text_style};
use zgui_text_style::{BreakingKey, ShapingKey};

/// What a style change costs the text pipeline.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TextWork {
    /// No glyph moves and no line falls differently.
    None,
    /// The lines have to be laid out again, reusing the glyphs already produced.
    Rebreak,
    /// The text has to be shaped again, and broken again after that.
    Reshape,
}

/// One element's text keys, as of the last time it was styled.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
struct TextKeys {
    /// Everything that decides which glyphs exist.
    shaping: ShapingKey,
    /// Everything that decides only where the lines fall.
    breaking: BreakingKey,
}

impl TextKeys {
    /// The keys of one computed style.
    fn of(style: &ComputedStyle) -> Self {
        let text = text_style(style);
        let paragraph = paragraph_style(style);
        // Both halves of each key are folded together, because an element's text is laid out
        // under both and a change to either is a change to the whole.
        Self {
            shaping: ShapingKey(ShapingKey::of(&text).0 ^ ShapingKey::of_paragraph(&paragraph).0),
            breaking: BreakingKey(
                BreakingKey::of(&text).0 ^ BreakingKey::of_paragraph(&paragraph).0,
            ),
        }
    }
}

/// The text keys of every element that has been styled.
///
/// Owned here rather than as a column on the document, because nothing outside damage translation
/// reads them: they exist only to be compared against the next restyle's.
#[derive(Default)]
pub struct TextKeyStore {
    /// Keys by element.
    keys: FxHashMap<NodeKey, TextKeys>,
    /// How many keys were left after the last sweep, which is what the next one is scheduled off.
    swept_to: usize,
}

/// The size beneath which sweeping is never worth what it costs.
const SWEEP_FLOOR: usize = 64;

impl TextKeyStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `style`'s keys for `node` and reports what the change from the previous ones costs.
    ///
    /// An element with no previous keys reports a re-shape: it has never been shaped, so there is
    /// nothing to reuse.
    pub fn record(&mut self, node: NodeKey, style: &ComputedStyle) -> TextWork {
        let keys = TextKeys::of(style);
        match self.keys.insert(node, keys) {
            Some(previous) if previous.shaping != keys.shaping => TextWork::Reshape,
            Some(previous) if previous.breaking != keys.breaking => TextWork::Rebreak,
            Some(_) => TextWork::None,
            None => TextWork::Reshape,
        }
    }

    /// Drops the keys of every element that is no longer in `store`'s document, and reports whether
    /// it swept at all.
    ///
    /// Nothing tells this store when an element goes away, so without a sweep a document that
    /// churns through rows keeps a comparison record for every row it has ever held. An element
    /// that has left the document can never be styled again without being put back — and being put
    /// back is a fresh cascade whose first comparison should report a re-shape anyway — so dropping
    /// its record changes no answer, while keeping it costs that memory for ever.
    ///
    /// Leaving the document is the test rather than the key failing to resolve, because a detached
    /// element's record can outlive its detachment: a key that no longer resolves is also swept, by
    /// the same test, since a record that is not there is not in the document either.
    ///
    /// A sweep reads every recorded key, so one per frame would charge a whole document's worth of
    /// work to a frame that restyled one element. Instead it runs only once the store has doubled
    /// since the last sweep left it: each sweep is then paid for by at least as many insertions as
    /// it examines, which is a constant per element added, while what a churning document holds
    /// stays within a factor of two of what it uses.
    pub fn retire(&mut self, store: &DocumentStore) -> bool {
        if self.keys.len() <= SWEEP_FLOOR.max(self.swept_to.saturating_mul(2)) {
            return false;
        }
        self.keys.retain(|key, _| {
            store
                .get(*key)
                .is_some_and(|node| node.has_flags(NodeFlags::IN_DOCUMENT))
        });
        self.swept_to = self.keys.len();
        true
    }

    /// How many elements have recorded keys.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether no element has recorded keys.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;
    use zgui_css::values::font::{FontSize, FontSizeExt};
    use zgui_css::values::text::TextAlignKeyword;
    use zgui_dom::{Document, NodeKey, NodeKind};
    use zgui_geom::CssPx;
    use zgui_interned::ElementName;

    use super::{TextKeyStore, TextWork};

    /// One element of a real document, which is where a node key comes from.
    fn element() -> (Document, NodeKey) {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let key = document.store().key_of(root);
        (document, key)
    }

    #[test]
    fn a_first_style_has_nothing_to_reuse() {
        let (_document, key) = element();
        let mut store = TextKeyStore::new();
        let style = StyleDraft::initial().build();
        assert_eq!(store.record(key, &style), TextWork::Reshape);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn re_aligning_a_paragraph_costs_a_break_and_not_a_shape() {
        let (_document, key) = element();
        let mut store = TextKeyStore::new();
        let before = StyleDraft::initial().build();
        assert_eq!(store.record(key, &before), TextWork::Reshape);

        let mut draft = StyleDraft::from_style(&before);
        draft.inherited_text().text_align = TextAlignKeyword::Center;
        let after = draft.build();
        assert_eq!(store.record(key, &after), TextWork::Rebreak);
    }

    #[test]
    fn changing_the_font_size_costs_a_shape() {
        let (_document, key) = element();
        let mut store = TextKeyStore::new();
        let before = StyleDraft::initial().build();
        store.record(key, &before);

        let mut draft = StyleDraft::from_style(&before);
        draft.font().font_size = FontSize::for_px(CssPx(20.0));
        let after = draft.build();
        assert_eq!(store.record(key, &after), TextWork::Reshape);
    }

    #[test]
    fn a_style_that_moves_nothing_the_text_pipeline_reads_costs_nothing() {
        let (_document, key) = element();
        let mut store = TextKeyStore::new();
        let style = StyleDraft::initial().build();
        store.record(key, &style);
        assert_eq!(store.record(key, &style), TextWork::None);
    }
}
