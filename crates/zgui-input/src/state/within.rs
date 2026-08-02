//! Moving one state bit from the path it was on to the path it is on now.
//!
//! `:hover` is not a property of one element. The pointer is over the button, and it is therefore
//! also over the toolbar the button is in and the surface the toolbar is on, so the bit is written
//! up the whole path — and moving the pointer from one button to its neighbour must not rewrite
//! the ancestors both of them share. That difference is what this module computes, and it is the
//! difference between one pointer move costing two restyles and costing the depth of the document
//! twice over.

use zgui_dom::{Document, NodeKey, StyleFilter};
use zgui_vocab::UiState;

use smallvec::SmallVec;

/// Which elements gained a bit and which lost it.
///
/// Both lists are in the order they appear on their own path, so the elements that gained the bit
/// run from the root downwards — which is the order an event announcing the change is delivered
/// in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Moved {
    /// The elements the bit was turned on for.
    pub entered: SmallVec<[NodeKey; 4]>,
    /// The elements the bit was turned off for.
    pub left: SmallVec<[NodeKey; 4]>,
}

impl Moved {
    /// Whether nothing changed at all, which is what an unmoved pointer costs.
    pub fn is_empty(&self) -> bool {
        self.entered.is_empty() && self.left.is_empty()
    }

    /// How many elements were written.
    pub fn len(&self) -> usize {
        self.entered.len() + self.left.len()
    }
}

/// Turns `bit` off along `from`, on along `to`, and leaves the elements on both alone.
///
/// One batch, so the whole move costs the document a single end-of-batch pass however many
/// elements it touched. An element on both paths is not written, which means it is not recorded,
/// not marked, and never seen by a restyle — the shared ancestors of two neighbouring rows are
/// most of both paths, so this is where the cost of a pointer moving across a list actually goes.
///
/// # Panics
///
/// Panics if the document is poisoned by an earlier batch that unwound.
pub fn move_bit(
    document: &Document,
    filter: &dyn StyleFilter,
    bit: UiState,
    from: &[NodeKey],
    to: &[NodeKey],
) -> Moved {
    let mut moved = Moved::default();
    if from == to {
        return moved;
    }
    document
        .edit(filter, |edit| {
            for key in from {
                if to.contains(key) {
                    continue;
                }
                let Some(index) = edit.document().store().index_of(*key) else {
                    continue;
                };
                edit.set_state(index, bit, false);
                moved.left.push(*key);
            }
            for key in to {
                if from.contains(key) {
                    continue;
                }
                let Some(index) = edit.document().store().index_of(*key) else {
                    continue;
                };
                edit.set_state(index, bit, true);
                moved.entered.push(*key);
            }
        })
        .expect("the document is not poisoned");
    moved
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters, NodeKey};
    use zgui_interned::ElementName;
    use zgui_vocab::UiState;

    use super::move_bit;

    /// `root > branch > (left, right)`, as keys.
    fn tree() -> (Document, [NodeKey; 4]) {
        let document = Document::new();
        let indices = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let branch = edit.create_element(ElementName::new("branch"));
                edit.insert_before(root, branch, None);
                let left = edit.create_element(ElementName::new("left"));
                edit.insert_before(branch, left, None);
                let right = edit.create_element(ElementName::new("right"));
                edit.insert_before(branch, right, None);
                [root, branch, left, right]
            })
            .expect("not poisoned");
        let keys = indices.map(|index| document.store().key_of(index));
        (document, keys)
    }

    /// Whether `key` carries `bit`.
    fn has(document: &Document, key: NodeKey, bit: UiState) -> bool {
        let index = document.store().index_of(key).expect("a live node");
        document.store().core(index).ui_state().contains(bit)
    }

    #[test]
    fn moving_between_siblings_writes_the_two_that_changed_and_not_their_ancestors() {
        let (document, [root, branch, left, right]) = tree();
        let onto_left = move_bit(
            &document,
            &EverythingMatters,
            UiState::HOVER,
            &[],
            &[root, branch, left],
        );
        assert_eq!(onto_left.entered.as_slice(), &[root, branch, left]);
        assert!(onto_left.left.is_empty());

        let across = move_bit(
            &document,
            &EverythingMatters,
            UiState::HOVER,
            &[root, branch, left],
            &[root, branch, right],
        );
        assert_eq!(
            across.left.as_slice(),
            &[left],
            "only the sibling that was left"
        );
        assert_eq!(
            across.entered.as_slice(),
            &[right],
            "only the sibling that was entered"
        );
        assert_eq!(across.len(), 2, "and nothing else was written at all");

        assert!(has(&document, root, UiState::HOVER));
        assert!(has(&document, branch, UiState::HOVER));
        assert!(!has(&document, left, UiState::HOVER));
        assert!(has(&document, right, UiState::HOVER));
    }

    #[test]
    fn leaving_the_document_clears_the_whole_path() {
        let (document, [root, branch, left, _]) = tree();
        move_bit(
            &document,
            &EverythingMatters,
            UiState::HOVER,
            &[],
            &[root, branch, left],
        );
        let cleared = move_bit(
            &document,
            &EverythingMatters,
            UiState::HOVER,
            &[root, branch, left],
            &[],
        );
        assert_eq!(cleared.left.len(), 3);
        assert!(!has(&document, root, UiState::HOVER));
    }

    #[test]
    fn an_unmoved_pointer_writes_nothing() {
        let (document, [root, branch, left, _]) = tree();
        let path = [root, branch, left];
        move_bit(&document, &EverythingMatters, UiState::HOVER, &[], &path);
        let again = move_bit(&document, &EverythingMatters, UiState::HOVER, &path, &path);
        assert!(again.is_empty());
    }

    #[test]
    fn an_element_that_has_been_removed_is_skipped_rather_than_panicking() {
        // A path is a snapshot, so an element on it can be gone by the time the write happens —
        // a handler that removed a row while the pointer was still leaving it. The write for the
        // removed element is dropped and the rest of the path is still written, which is what
        // keeps one removal from leaving `:hover` stuck on every ancestor.
        let (mut document, [root, branch, left, right]) = tree();
        let removed = right;
        let index = document.store().index_of(removed).expect("a live node");
        document
            .edit(&EverythingMatters, |edit| edit.remove(index))
            .expect("not poisoned");
        zgui_dom::arena::recycle::end_frame(&mut document);
        assert!(document.store().index_of(removed).is_none());

        let moved = move_bit(
            &document,
            &EverythingMatters,
            UiState::HOVER,
            &[],
            &[root, branch, removed, left],
        );
        assert_eq!(moved.entered.as_slice(), &[root, branch, left]);
    }
}
