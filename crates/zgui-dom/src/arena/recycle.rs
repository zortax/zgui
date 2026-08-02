//! Ending a frame: dropping what was removed during it, and returning its slots.
//!
//! Removal is deferred by a frame on purpose. A node removed part-way through a frame is still
//! readable through its slot number for the rest of that frame, because things that legitimately
//! still hold it have not run yet — the accessibility walk over the node's boxes, a pending binding
//! asking for its bounds, the style engine's own snapshot map. Dropping the record at the moment of
//! removal would leave every one of those reading a slot that no longer holds what it was promised.
//!
//! The other half is what makes slot numbers safe to use as names inside a frame at all: a slot
//! freed during a frame is not handed out again until the frame ends, so a number cannot come to
//! mean a different node part-way through one.
//!
//! # What is dropped, and what is not
//!
//! A subtree that was taken out of the document and is still out of it when the frame ends. That is
//! narrower than "everything [`Edit::remove`](crate::Edit::remove) was called on": a subtree put
//! back before the frame ends was never really removed, and the frame's own stages are entitled to
//! do exactly that. It is also wider than "the roots that were removed", because a removal takes
//! everything below the root with it, and a descendant that was moved elsewhere in the meantime is
//! reached from its new parent instead and so is not.
//!
//! A node that was built detached and never linked in is not dropped either, whether or not it is
//! ever used: nothing distinguishes it from a subtree still being assembled, and the code that
//! built it holds its name.

mod subtree;

use crate::arena::document::Document;

/// Ends a frame: drops the nodes removed during it and returns their slots for reuse.
///
/// Call this once per frame, after every stage that could still hold a name from the frame being
/// ended. Calling it early is what the whole deferral exists to prevent; calling it late costs the
/// memory of whatever was removed for one extra frame, and skipping it entirely keeps every record,
/// key and side-table row of every node the document has ever removed for as long as the document
/// lives.
pub fn end_frame(document: &mut Document) {
    subtree::drop_detached(document);
    let store = document.store_mut();
    store.arena_mut().recycle();
    store.columns_mut().compact();
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;

    use super::end_frame;
    use crate::arena::document::Document;
    use crate::mutate::filter::EverythingMatters;
    use crate::node::kind::NodeKind;

    #[test]
    fn ending_a_frame_returns_emptied_column_pages() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let key = document.store().key_of(root);
        *document.store_mut().columns_mut().text.get_mut(key) = Some("hello".into());
        assert_eq!(document.store().columns().allocated_pages(), 1);

        document.store_mut().columns_mut().text.clear(key);
        end_frame(&mut document);
        assert_eq!(document.store().columns().allocated_pages(), 0);
    }

    #[test]
    fn ending_a_frame_leaves_live_nodes_alone() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let held = core::ptr::from_ref(document.store().core(root));
        end_frame(&mut document);
        assert_eq!(held, core::ptr::from_ref(document.store().core(root)));
    }

    #[test]
    fn a_node_built_detached_and_never_linked_in_survives_the_frame() {
        // The code that built it holds its name and is entitled to link it in next frame. Nothing
        // here can tell that apart from a subtree still being assembled, so neither is touched.
        let mut document = Document::new();
        let loose = document.detached(NodeKind::Element, ElementName::new("box"));
        end_frame(&mut document);
        assert!(document.store().try_core(loose).is_some());
    }

    #[test]
    fn a_removed_subtree_put_back_before_the_frame_ends_is_kept() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let row = document.append(root, NodeKind::Element, ElementName::new("li"));
        let text = document.append(row, NodeKind::Text, ElementName::new("#text"));

        document
            .edit(&EverythingMatters, |batch| batch.remove(row))
            .expect("the document is not poisoned");
        document
            .edit(&EverythingMatters, |batch| {
                batch.insert_before(root, row, None);
            })
            .expect("the document is not poisoned");

        end_frame(&mut document);
        assert!(document.store().try_core(row).is_some());
        assert!(document.store().try_core(text).is_some());
    }
}
