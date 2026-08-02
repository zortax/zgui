//! Stopping the window scrolling without moving it.
//!
//! A modal surface has to stop the page behind it moving, and the obvious way to do that is to
//! restyle the root so that it no longer scrolls. That is the wrong way, and the failure is
//! visible rather than subtle: a box that is not a scroll container has no offset composed into
//! its descendants, so the page snaps to the top the instant the surface opens and snaps back when
//! it closes.
//!
//! So nothing is restyled. The window keeps every scroll container it had, keeps the offset each
//! one is at, keeps the width its content wrapped to and keeps the gutter its scrollbar occupies.
//! What a freeze takes away is only the ability to *move*: the window's own container is dropped
//! out of every scroll chain, so a wheel, a trackpad, a key, an accessibility action and a scroll
//! a view asks for all leave it exactly where it was.
//!
//! Only the window's own container freezes. A list inside the page keeps its own scrolling, and a
//! surface opened over the page keeps its own too — which is what makes a long dialog usable while
//! the page behind it is still.

use zgui_dom::NodeKey;

use crate::window::Window;

impl Window {
    /// Stops, or lets go of, this window's own scrolling.
    ///
    /// Idempotent: freezing a window that is already frozen changes nothing, and thawing one that
    /// was never frozen changes nothing either. Whoever needs a count — a dialog opened from a
    /// dialog is two holders — keeps it and calls here on the first hold and the last release.
    pub fn freeze_scrolling(&mut self, frozen: bool) {
        self.scroll_frozen = frozen;
        // A window frozen in the middle of a glide would keep arriving where the wheel had asked
        // for, one frame at a time, with a modal surface already over it.
        if let Some(container) = self.frozen_container() {
            self.scroll.borrow_mut().halt(container);
        }
    }

    /// Whether this window's own scrolling is frozen.
    pub fn scrolling_frozen(&self) -> bool {
        self.scroll_frozen
    }

    /// The element whose scrolling a freeze stops, which is the window's root.
    ///
    /// `None` for a window whose root has no document key, which is a window that has not been
    /// built; nothing can be scrolling in it either.
    pub(crate) fn frozen_container(&self) -> Option<NodeKey> {
        self.scroll_frozen
            .then(|| zgui_view_dom::id::to_document(self.dom.root_node()))
            .flatten()
    }

    /// Whether one container is the frozen one, and therefore may not be moved.
    pub(crate) fn is_frozen(&self, container: NodeKey) -> bool {
        self.frozen_container() == Some(container)
    }
}
