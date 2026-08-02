//! Changing what an element is called by: its classes, its identifier and its attributes.
//!
//! These are the changes selector matching depends on most directly, so each of them records what
//! the element looked like before. The record is what lets the style engine work out that
//! `.item.hot + .label` no longer matches: the element it re-matches is the one that changed, and
//! the element whose style changed is the one after it.
//!
//! # The change that provably matters to nothing
//!
//! A component library drives its variants from classes and data attributes, and most of them are
//! named by no selector at all. Such a change cannot alter any computed value anywhere, so it is
//! applied and nothing else happens: no record, no ancestor marked, no entry into the style engine.
//! The answer comes from the rule set through the filter handed to the batch, and it is asked in the
//! direction that is safe to get wrong — a filter that says "this might matter" costs a restyle,
//! and one that wrongly says "this cannot" costs a wrong colour with nothing to notice it by.

use zgui_bits::Dirty;
use zgui_interned::{AttrName, ClassName, Ident};
use zgui_vocab::SharedString;

use crate::id::node_key::NodeIndex;
use crate::mutate::ancestors;
use crate::mutate::edit::Edit;

impl Edit<'_> {
    /// Replaces `node`'s classes.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn set_classes(&mut self, node: NodeIndex, classes: &[ClassName]) {
        let held: Vec<ClassName> = self.classes_of(node);
        if held.as_slice() == classes {
            return;
        }
        let filter = self.filter;
        let matters = held
            .iter()
            .chain(classes)
            .copied()
            .filter(|class| held.contains(class) != classes.contains(class))
            .any(|class| filter.names_class(class));

        if matters {
            let (store, batch) = self.parts();
            batch.snapshots.record_attributes(store, node);
            batch.snapshots.note_class_changed(store, node);
        }
        self.store().write_classes(node, classes);
        if matters {
            ancestors::mark(self.store(), node, Dirty::RESTYLE);
        }
    }

    /// Adds one class to `node`, if it does not have it already.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn add_class(&mut self, node: NodeIndex, class: ClassName) {
        let mut classes = self.classes_of(node);
        if classes.contains(&class) {
            return;
        }
        classes.push(class);
        self.set_classes(node, &classes);
    }

    /// Removes one class from `node`, if it has it.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn remove_class(&mut self, node: NodeIndex, class: ClassName) {
        let mut classes = self.classes_of(node);
        let Some(position) = classes.iter().position(|held| *held == class) else {
            return;
        };
        classes.remove(position);
        self.set_classes(node, &classes);
    }

    /// Sets or clears `node`'s identifier.
    ///
    /// Always takes the full path: what a rule set names is asked about classes and attributes,
    /// which a component library changes constantly, and not about identifiers, which are written
    /// once when an element is built.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn set_id(&mut self, node: NodeIndex, id: Option<Ident>) {
        if self.store().core(node).id_attr() == id {
            return;
        }
        let (store, batch) = self.parts();
        batch.snapshots.record_attributes(store, node);
        batch.snapshots.note_id_changed(store, node);
        store.write_id(node, id);
        ancestors::mark(store, node, Dirty::RESTYLE);
    }

    /// Sets or clears an attribute of `node` other than `id` and `class`.
    ///
    /// Those two are not attributes here — they live in the node record, because matching asks
    /// about them far more often than about anything else — and are written through
    /// [`Edit::set_id`] and [`Edit::set_classes`].
    ///
    /// Writing an attribute re-asks the installed link resolver whether this element is a link, in
    /// the same call, because that is what makes `:link` and `:visited` invalidate: the answer is
    /// held in the element's interaction state, and the style engine notices a state change and
    /// nothing else.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn set_attribute(&mut self, node: NodeIndex, name: AttrName, value: Option<SharedString>) {
        let matters = self.filter.names_attr(name);
        if matters {
            let (store, batch) = self.parts();
            batch.snapshots.record_attributes(store, node);
            batch
                .snapshots
                .note_attr_changed(store, node, name.as_str());
        }
        let store = self.store();
        store.write_attribute(node, name, value);
        if matters {
            ancestors::mark(store, node, Dirty::RESTYLE);
        }
    }

    /// `node`'s classes, copied out so that the store can be written while they are held.
    fn classes_of(&mut self, node: NodeIndex) -> Vec<ClassName> {
        self.store()
            .classes_of(node)
            .iter()
            .map(|atom| ClassName::new(atom.as_ref()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_interned::{AttrName, ClassName, ElementName};
    use zgui_vocab::SharedString;

    use crate::arena::document::Document;
    use crate::mutate::filter::{EverythingMatters, StyleFilter};
    use crate::node::kind::NodeKind;

    /// A rule set in which only `.hot` is named by anything.
    struct OnlyHot;

    impl StyleFilter for OnlyHot {
        fn is_disabled(&self) -> bool {
            false
        }

        fn names_class(&self, class: ClassName) -> bool {
            class.as_str() == "hot"
        }

        fn names_attr(&self, _attr: AttrName) -> bool {
            false
        }
    }

    /// A document with one element, and that element.
    fn one() -> (Document, crate::id::node_key::NodeIndex) {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        (document, root)
    }

    #[test]
    fn a_class_no_selector_names_is_applied_and_nothing_else_happens() {
        let (document, root) = one();
        document
            .edit(&OnlyHot, |edit| edit.add_class(root, ClassName::new("v2")))
            .expect("not poisoned");

        assert_eq!(document.store().classes_of(root).len(), 1);
        assert_eq!(document.pending_snapshots(), 0, "no record was taken");
        assert!(
            document.store().core(root).dirty().own().is_clean(),
            "no computed value can have changed, so a restyle here is provably wasted work"
        );
    }

    #[test]
    fn a_class_a_selector_does_name_takes_the_full_path() {
        let (document, root) = one();
        document
            .edit(&OnlyHot, |edit| edit.add_class(root, ClassName::new("hot")))
            .expect("not poisoned");

        assert_eq!(document.pending_snapshots(), 1);
        assert!(
            document
                .store()
                .core(root)
                .dirty()
                .own()
                .contains(Dirty::RESTYLE)
        );
    }

    #[test]
    fn removing_a_class_a_selector_names_takes_the_full_path_too() {
        let (mut document, root) = one();
        document.set_classes(root, &[ClassName::new("hot"), ClassName::new("v2")]);
        document
            .edit(&OnlyHot, |edit| {
                edit.remove_class(root, ClassName::new("v2"))
            })
            .expect("not poisoned");
        assert_eq!(document.pending_snapshots(), 0);

        document
            .edit(&OnlyHot, |edit| {
                edit.remove_class(root, ClassName::new("hot"))
            })
            .expect("not poisoned");
        assert_eq!(document.pending_snapshots(), 1);
    }

    #[test]
    fn an_attribute_no_selector_names_is_applied_and_nothing_else_happens() {
        let (document, root) = one();
        document
            .edit(&OnlyHot, |edit| {
                edit.set_attribute(
                    root,
                    AttrName::new("data-variant"),
                    Some(SharedString::from("solid")),
                );
            })
            .expect("not poisoned");

        assert_eq!(
            document.node(root).attr("data-variant").map(|v| v.as_str()),
            Some("solid")
        );
        assert_eq!(document.pending_snapshots(), 0);
        assert!(document.store().core(root).dirty().own().is_clean());
    }

    #[test]
    fn the_default_filter_proves_nothing_irrelevant_so_every_change_is_recorded() {
        let (document, root) = one();
        document
            .edit(&EverythingMatters, |edit| {
                edit.add_class(root, ClassName::new("v2"));
            })
            .expect("not poisoned");
        assert_eq!(document.pending_snapshots(), 1);
    }
}
