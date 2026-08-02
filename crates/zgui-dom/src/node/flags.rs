//! Per-node bits that are neither interaction state nor style-engine bookkeeping.

use crate::plain_data;

bitflags::bitflags! {
    /// Structural facts about a node that no style rule and no worker thread writes.
    ///
    /// These are written between frames, under an exclusive borrow of the document, which is why
    /// they can sit in a plain cell beside fields that need atomics.
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug)]
    pub struct NodeFlags: u16 {
        /// This node is the root element of its document.
        const IS_ROOT     = 1 << 0;
        /// This node is attached to the document rather than detached in a fragment.
        const IN_DOCUMENT = 1 << 1;
        /// This node's content comes from outside the document — an image, a canvas, a video —
        /// so its intrinsic size is asked of the content and not derived from children.
        const IS_REPLACED = 1 << 2;
        /// This node can hold keyboard focus.
        const FOCUSABLE   = 1 << 3;
    }
}

plain_data!(NodeFlags);

#[cfg(test)]
mod tests {
    use super::NodeFlags;

    #[test]
    fn the_flag_word_is_two_bytes_and_the_default_is_detached() {
        assert_eq!(size_of::<NodeFlags>(), 2);
        assert_eq!(NodeFlags::default(), NodeFlags::empty());
    }

    #[test]
    fn the_flags_are_a_set() {
        let flags = NodeFlags::IS_ROOT | NodeFlags::IN_DOCUMENT;
        assert!(flags.contains(NodeFlags::IS_ROOT));
        assert!(!flags.contains(NodeFlags::FOCUSABLE));
        assert_eq!(flags | NodeFlags::IS_ROOT, flags);
    }
}
