//! Which element is receiving a pointer regardless of where that pointer is.
//!
//! Pressing a slider's thumb and dragging past the edge of the slider has to keep moving the
//! thumb. That is pointer capture, and it is this framework's own rather than the operating
//! system's: no portable pointer grab exists — one desktop does not implement locking the pointer
//! and another does not implement confining it — and a grab that did work would take the pointer
//! away from the rest of the desktop for the sake of one widget. So capture is a routing rule, not
//! a request to the window system: while it is held, every event from that pointer is aimed at the
//! capturing element wherever the pointer actually is.

use smallvec::SmallVec;
use zgui_dom::NodeKey;
use zgui_vocab::PointerId;

/// Which element each pointer is captured by.
///
/// ```
/// use zgui_input::PointerCapture;
/// use zgui_vocab::PointerId;
/// # fn example(thumb: zgui_dom::NodeKey) {
/// let mut capture = PointerCapture::default();
/// capture.set(PointerId::MOUSE, thumb);
/// assert_eq!(capture.of(PointerId::MOUSE), Some(thumb));
///
/// capture.release(PointerId::MOUSE);
/// assert_eq!(capture.of(PointerId::MOUSE), None);
/// # }
/// ```
#[derive(Clone, Debug, Default)]
pub struct PointerCapture {
    /// One entry per captured pointer. Two at once is a drag with two fingers, not an error.
    held: SmallVec<[(PointerId, NodeKey); 2]>,
}

impl PointerCapture {
    /// Routes `pointer` to `node` until it is released.
    ///
    /// Capturing a pointer that is already captured replaces the holder, which is what a handler
    /// that captures on a press expects when a press arrives while another element still holds a
    /// stale capture.
    pub fn set(&mut self, pointer: PointerId, node: NodeKey) {
        match self.held.iter_mut().find(|(id, _)| *id == pointer) {
            Some((_, held)) => *held = node,
            None => self.held.push((pointer, node)),
        }
    }

    /// Which element `pointer` is captured by, if any.
    pub fn of(&self, pointer: PointerId) -> Option<NodeKey> {
        self.held
            .iter()
            .find(|(id, _)| *id == pointer)
            .map(|(_, node)| *node)
    }

    /// Ends `pointer`'s capture, and says whether there was one.
    pub fn release(&mut self, pointer: PointerId) -> bool {
        let before = self.held.len();
        self.held.retain(|(id, _)| *id != pointer);
        self.held.len() != before
    }

    /// Ends every capture held by `node`, and says whether there were any.
    ///
    /// The unmount path: an element that is removed while it holds the pointer would otherwise
    /// keep every subsequent event aimed at a node that no longer exists, and the interaction
    /// would be unrecoverable without another press.
    pub fn release_node(&mut self, node: NodeKey) -> bool {
        let before = self.held.len();
        self.held.retain(|(_, held)| *held != node);
        self.held.len() != before
    }

    /// Whether any pointer is captured.
    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// Ends every capture.
    pub fn clear(&mut self) {
        self.held.clear();
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::Document;
    use zgui_interned::ElementName;
    use zgui_vocab::PointerId;

    use super::PointerCapture;

    /// Two elements of one document.
    fn two() -> (Document, zgui_dom::NodeKey, zgui_dom::NodeKey) {
        let document = Document::new();
        let (first, second) = document
            .edit(&zgui_dom::EverythingMatters, |edit| {
                let first = edit.create_element(ElementName::new("first"));
                edit.insert_before(document.document_index(), first, None);
                let second = edit.create_element(ElementName::new("second"));
                edit.insert_before(document.document_index(), second, None);
                (first, second)
            })
            .expect("not poisoned");
        let keys = (
            document.store().key_of(first),
            document.store().key_of(second),
        );
        (document, keys.0, keys.1)
    }

    #[test]
    fn two_fingers_capture_two_elements_at_once() {
        let (_document, first, second) = two();
        let mut capture = PointerCapture::default();
        capture.set(PointerId::MOUSE, first);
        capture.set(PointerId::new(2), second);

        assert_eq!(capture.of(PointerId::MOUSE), Some(first));
        assert_eq!(capture.of(PointerId::new(2)), Some(second));

        assert!(capture.release(PointerId::MOUSE));
        assert_eq!(
            capture.of(PointerId::new(2)),
            Some(second),
            "releasing one finger must not release the other"
        );
    }

    #[test]
    fn an_element_that_goes_away_releases_whatever_it_held() {
        let (_document, first, second) = two();
        let mut capture = PointerCapture::default();
        capture.set(PointerId::MOUSE, first);
        capture.set(PointerId::new(2), first);
        capture.set(PointerId::new(3), second);

        assert!(capture.release_node(first));
        assert!(capture.of(PointerId::MOUSE).is_none());
        assert!(capture.of(PointerId::new(2)).is_none());
        assert_eq!(capture.of(PointerId::new(3)), Some(second));
        assert!(!capture.is_empty());

        assert!(
            !capture.release_node(first),
            "and says so when there were none"
        );
    }

    #[test]
    fn capturing_again_replaces_the_holder_rather_than_adding_one() {
        let (_document, first, second) = two();
        let mut capture = PointerCapture::default();
        capture.set(PointerId::MOUSE, first);
        capture.set(PointerId::MOUSE, second);
        assert_eq!(capture.of(PointerId::MOUSE), Some(second));
        capture.clear();
        assert!(capture.is_empty());
    }
}
