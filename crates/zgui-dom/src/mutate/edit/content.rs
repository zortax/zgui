//! Text, meaning, and the properties selectors cannot see.
//!
//! None of these can change what a selector matches — with one exception, which is the whole
//! reason this module is not a set of one-line column writes.
//!
//! **Text has a selector-visible side effect and it belongs to the parent.** An element counts as
//! empty when it has no element child *and* no text child of non-zero length, so `<p>{message}</p>`
//! under `p:empty { display: none }` changes what it matches when the message goes from nothing to
//! something — with no node inserted, no node removed, and therefore no visit to the protocol that
//! handles child lists. So a text change that flips that predicate records the parent's own
//! emptiness. In the common case, where the flag says no rule anywhere asks whether this element is
//! empty, that is one atomic load and a return.
//!
//! **What changes here is invisible to the style engine, so it is marked directly.** A text edit
//! changes an accessible name, and the comparison that derives accessibility damage runs only over
//! the elements the style engine restyled — which a text edit is not one of. Without a direct mark
//! a counter in a label updates on screen and never in the accessibility tree.

use zgui_bits::Dirty;
use zgui_vocab::{PropKey, PropValue, Semantics};

use crate::id::node_key::NodeIndex;
use crate::mutate::ancestors;
use crate::mutate::edit::Edit;
use crate::node::kind::NodeKind;

impl Edit<'_> {
    /// Replaces the text `node` holds.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document, or if it is not a text node.
    pub fn set_text(&mut self, node: NodeIndex, text: &str) {
        assert_eq!(
            self.store().core(node).kind(),
            NodeKind::Text,
            "only a text node holds text"
        );
        let was_blank = crate::text::node::text_of(self.store(), node).is_none_or(str::is_empty);
        if crate::text::node::text_of(self.store(), node) == Some(text) {
            return;
        }

        if was_blank != text.is_empty()
            && let Some(parent) = self.store().core(node).parent()
        {
            let (store, batch) = self.parts();
            batch.structure.record_emptiness_change(store, parent);
        }

        crate::text::node::set_text(self.store(), node, text);
        ancestors::mark(self.store(), node, Dirty::RESHAPE | Dirty::A11Y);
    }

    /// Sets or clears what `node` means, for the accessibility projection.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn set_semantics(&mut self, node: NodeIndex, semantics: Option<Semantics>) {
        let store = self.store();
        let key = store.key_of(node);
        *store.columns_mut().semantics.get_mut(key) = semantics.map(Box::new);
        ancestors::mark(store, node, Dirty::A11Y);
    }

    /// Sets or clears one of `node`'s imperative properties.
    ///
    /// These are deliberately invisible to selector matching. A text field's current value is the
    /// worked example: it changes on every keystroke, and were it an attribute every keystroke
    /// would take a record and invalidate every rule that could depend on one.
    ///
    /// # Two of them are read by the paint stage
    ///
    /// The properties an element's outlines are carried in are the exception, and they are the
    /// reason this is not one column write. Nothing about a drawing reaches the style engine, so
    /// the comparison that derives repaint damage cannot see a change to one: an icon swapped for
    /// another of the same size would keep its geometry, keep its style, compare identical in every
    /// stage that looks, and stay on the screen as it was. So the obligation is marked here.
    ///
    /// Outlines *appearing or disappearing* is a stronger change than that: whether an element
    /// draws decides what kind of piece its box produces, and that is decided while the box tree is
    /// built. Only that case rebuilds; changing the outlines of an element that already had some
    /// repaints where it stands.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn set_property(&mut self, node: NodeIndex, key: PropKey, value: Option<PropValue>) {
        let store = self.store();
        let slot = store.key_of(node);
        let paints = zgui_vocab::prop::drawing::paints(key.as_str());
        let custom = key.as_str() == zgui_vocab::prop::custom::ELEMENT;
        let drew = paints && crate::side::drawing::draws(store, slot);
        let properties = store.columns_mut().props.get_mut(slot);
        let watched = paints || custom;
        let before = watched.then(|| properties.get(key).cloned()).flatten();
        match value {
            Some(value) => {
                properties.set(key, value);
            }
            None => {
                properties.remove(key);
            }
        }
        let after = watched.then(|| properties.get(key).cloned()).flatten();
        let mut owed = Dirty::A11Y;
        if paints && before != after {
            owed |= Dirty::REPAINT;
            if drew != crate::side::drawing::draws(store, slot) {
                owed |= Dirty::REBUILD_BOX | Dirty::RELAYOUT;
            }
        }
        if custom && before != after {
            // The reference packs which implementation owns the box and two revisions; which of
            // the three moved decides what is owed. Ownership appearing or disappearing changes
            // what kind of box the element generates; a moved layout revision re-measures it; any
            // movement repaints it.
            owed |= Dirty::REPAINT;
            match (&before, &after) {
                (Some(PropValue::Integer(before)), Some(PropValue::Integer(after))) => {
                    if zgui_vocab::prop::custom::relayouts(*before, *after) {
                        owed |= Dirty::RELAYOUT;
                    }
                }
                _ => owed |= Dirty::REBUILD_BOX | Dirty::RELAYOUT,
            }
        }
        ancestors::mark(store, node, owed);
    }
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_interned::ElementName;

    use crate::arena::document::Document;
    use crate::mutate::filter::EverythingMatters;
    use crate::node::kind::NodeKind;

    #[test]
    fn a_text_change_marks_reshaping_and_the_accessible_name() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let text = document.append(root, NodeKind::Text, ElementName::new("#text"));

        document
            .edit(&EverythingMatters, |edit| edit.set_text(text, "Saved"))
            .expect("not poisoned");

        let owed = document.store().core(text).dirty().own();
        assert!(owed.contains(Dirty::RESHAPE));
        assert!(
            owed.contains(Dirty::A11Y),
            "without this a label's accessible name never follows its text"
        );
        assert_eq!(
            crate::text::node::text_of(document.store(), text),
            Some("Saved")
        );
    }

    #[test]
    fn writing_the_same_text_again_does_nothing() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let text = document.append(root, NodeKind::Text, ElementName::new("#text"));
        document
            .edit(&EverythingMatters, |edit| edit.set_text(text, "Saved"))
            .expect("not poisoned");
        document.store().core(text).dirty().clear_own(Dirty::all());

        document
            .edit(&EverythingMatters, |edit| edit.set_text(text, "Saved"))
            .expect("not poisoned");
        assert!(document.store().core(text).dirty().own().is_clean());
    }

    /// A drawing swapped for another of the same size moves nothing the style engine can see, so
    /// the obligation to repaint has to be recorded where the write happens or it is recorded
    /// nowhere and the old drawing stays on the screen.
    #[test]
    fn changing_the_outlines_an_element_draws_owes_a_repaint() {
        use zgui_vocab::prop::drawing;

        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("vector"),
        );
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_property(
                    root,
                    zgui_vocab::PropKey::new(drawing::PATHS),
                    Some(zgui_vocab::PropValue::from("M0 0 L8 0 L8 8 Z")),
                );
            })
            .expect("not poisoned");
        assert!(
            document
                .store()
                .core(root)
                .dirty()
                .own()
                .contains(Dirty::REBUILD_BOX),
            "an element that did not draw and now does produces a different kind of piece"
        );
        document.store().core(root).dirty().clear_own(Dirty::all());

        document
            .edit(&EverythingMatters, |edit| {
                edit.set_property(
                    root,
                    zgui_vocab::PropKey::new(drawing::PATHS),
                    Some(zgui_vocab::PropValue::from("M0 0 L8 8 L0 8 Z")),
                );
            })
            .expect("not poisoned");
        let owed = document.store().core(root).dirty().own();
        assert!(owed.contains(Dirty::REPAINT));
        assert!(
            !owed.contains(Dirty::REBUILD_BOX),
            "an element that drew and still draws must not replace every box in the document"
        );
    }

    /// A field's text changes on every keystroke and paints nothing by itself, so it must not.
    #[test]
    fn a_property_that_paints_nothing_owes_no_repaint() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("field"),
        );
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_property(
                    root,
                    zgui_vocab::PropKey::new("value"),
                    Some(zgui_vocab::PropValue::from("hello")),
                );
            })
            .expect("not poisoned");
        let owed = document.store().core(root).dirty().own();
        assert!(owed.contains(Dirty::A11Y));
        assert!(!owed.intersects(Dirty::REPAINT | Dirty::REBUILD_BOX));
    }

    /// Writing the same outlines again is not a change, and repainting for it would repaint on
    /// every frame a view re-ran without changing anything.
    #[test]
    fn writing_the_same_outlines_again_owes_no_repaint() {
        use zgui_vocab::prop::drawing;

        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("vector"),
        );
        let write = |document: &Document| {
            document
                .edit(&EverythingMatters, |edit| {
                    edit.set_property(
                        root,
                        zgui_vocab::PropKey::new(drawing::PATHS),
                        Some(zgui_vocab::PropValue::from("M0 0 L8 0 L8 8 Z")),
                    );
                })
                .expect("not poisoned");
        };
        write(&document);
        document.store().core(root).dirty().clear_own(Dirty::all());
        write(&document);
        assert!(
            !document
                .store()
                .core(root)
                .dirty()
                .own()
                .contains(Dirty::REPAINT)
        );
    }
}
