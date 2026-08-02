//! Which range of an editable node's text is selected.
//!
//! The runtime owns this for the same reason it owns scroll offsets and focus: it is state about
//! the document rather than a property of it, every question about it is asked from a view through
//! one seam, and nothing below the runtime may keep a second copy.
//!
//! Ranges are byte offsets into the text the node's subtree holds, which is what the view layer's
//! own signatures say and what an editing model works in.

use std::cell::RefCell;
use std::ops::Range;

use rustc_hash::FxHashMap;
use zgui_dom::{DocumentStore, NodeIndex, NodeKey};

/// The selected range of every node that has one.
///
/// A node with no entry has no selection, which is different from an empty one: an empty range is
/// a caret sitting between two characters and is the ordinary state of a focused field.
#[derive(Debug, Default)]
pub struct Selections {
    /// The ranges, by node.
    ranges: RefCell<FxHashMap<NodeKey, Range<usize>>>,
}

impl Selections {
    /// Nothing selected anywhere.
    pub fn new() -> Self {
        Self::default()
    }

    /// The range selected in a node.
    pub fn of(&self, node: NodeKey) -> Option<Range<usize>> {
        self.ranges.borrow().get(&node).cloned()
    }

    /// Selects a range of a node's text, clamped to how much text there is.
    ///
    /// Clamping rather than refusing, because the caller's offsets are as old as whatever it last
    /// read: a component that remembered a selection across an edit that shortened the value would
    /// otherwise have to re-check it before every write.
    pub fn set(&self, node: NodeKey, range: Range<usize>, length: usize) {
        let start = range.start.min(length);
        let end = range.end.clamp(start, length);
        self.ranges.borrow_mut().insert(node, start..end);
    }

    /// Forgets a node's selection, which is what unmounting one does.
    pub fn clear(&self, node: NodeKey) {
        self.ranges.borrow_mut().remove(&node);
    }

    /// How many nodes hold a selection.
    pub fn len(&self) -> usize {
        self.ranges.borrow().len()
    }

    /// Whether nothing anywhere is selected.
    pub fn is_empty(&self) -> bool {
        self.ranges.borrow().is_empty()
    }
}

/// How many bytes of text a node's subtree holds.
///
/// The whole subtree, because an editable element holds its text in text nodes — one per paragraph
/// — and a selection over it is expressed in offsets into all of them. The breaks between
/// paragraphs count as one byte each, so the offsets agree with the string those paragraphs join
/// into, which is the string every selection, event payload and accessibility range uses.
pub fn text_length(store: &DocumentStore, node: NodeIndex) -> usize {
    let mut length = 0;
    let mut paragraphs = 0;
    let mut stack = vec![node];
    while let Some(index) = stack.pop() {
        if let Some(text) = zgui_dom::text::text_of(store, index) {
            length += text.len();
            paragraphs += 1;
        }
        let mut child = store.core(index).first_child();
        while let Some(index) = child {
            stack.push(index);
            child = store.core(index).next_sibling();
        }
    }
    length + paragraphs.max(1) - 1
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters, NodeIndex};
    use zgui_interned::ElementName;

    use super::{Selections, text_length};

    /// A document whose editable element holds `paragraphs`, one text node each.
    fn field(paragraphs: &[&str]) -> (Document, NodeIndex) {
        let document = Document::new();
        let field = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let field = edit.create_element(ElementName::new("editor"));
                edit.insert_before(root, field, None);
                for paragraph in paragraphs {
                    let text = edit.create_text(paragraph);
                    edit.insert_before(field, text, None);
                }
                field
            })
            .expect("not poisoned");
        (document, field)
    }

    #[test]
    fn the_length_of_a_field_counts_one_byte_for_each_break_between_its_paragraphs() {
        let (document, field) = field(&["ab", "cd", "ef"]);
        assert_eq!(
            text_length(document.store(), field),
            8,
            "six bytes of text and two breaks"
        );
    }

    #[test]
    fn an_empty_field_is_zero_bytes_long_rather_than_underflowing() {
        let (document, field) = field(&[]);
        assert_eq!(text_length(document.store(), field), 0);
    }

    #[test]
    fn a_range_past_the_end_is_clamped_to_the_text_that_is_there() {
        let (document, field) = field(&["abc"]);
        let length = text_length(document.store(), field);
        let selections = Selections::new();
        let key = document.store().key_of(field);
        selections.set(key, 1..99, length);
        assert_eq!(selections.of(key), Some(1..3));
    }

    #[test]
    fn a_backwards_range_becomes_an_empty_one_at_its_start() {
        let (document, field) = field(&["abcdef"]);
        let length = text_length(document.store(), field);
        let selections = Selections::new();
        let key = document.store().key_of(field);
        // Written the way a caller with a backwards drag has it: the range reads high to low.
        #[allow(clippy::reversed_empty_ranges)]
        let backwards = 4..2;
        selections.set(key, backwards, length);
        assert_eq!(
            selections.of(key),
            Some(4..4),
            "a caret, rather than a range that reads backwards"
        );
    }

    #[test]
    fn a_node_nobody_selected_anything_in_has_no_selection() {
        let (document, field) = field(&["abc"]);
        let selections = Selections::new();
        let key = document.store().key_of(field);
        assert_eq!(selections.of(key), None);
        selections.set(key, 0..0, 3);
        assert_eq!(
            selections.of(key),
            Some(0..0),
            "and an empty selection is a caret, not the absence of one"
        );
        selections.clear(key);
        assert_eq!(selections.of(key), None);
    }
}
