//! Which of a node's measurements something is watching, and what it was last told.
//!
//! A view can ask to be told when an element's border box, content size, scroll offset or
//! scrollport changes. Answering that means two things per frame: deciding, for every node whose
//! fragment moved, whether anything is watching it — which has to be free, because it is asked of
//! every node — and deciding whether what is watching it would learn anything new.
//!
//! Both answers are here, and nothing else is. The delivery channels and the reference-counted
//! registry entries behind them live in the runtime: a channel is an erased closure and an entry
//! holds a shared signal, and a column holding either fails the store's `Sync` assertion, which
//! exists to reject exactly that. So the runtime's registry is the authority on *who* is watching,
//! and this column is the per-node probe and the previous-value cache.

use zgui_geom::{Device, DevicePx, Point, Rect, Size};

bitflags::bitflags! {
    /// Which measurements of one node are being watched.
    ///
    /// The bit positions correspond, by position, to the view layer's own enumeration of what can
    /// be observed. The correspondence is asserted where both are in scope, which is not here:
    /// this crate sits below the one that names them.
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug)]
    pub struct ObservedMask: u8 {
        /// The border box, in device pixels.
        const BORDER_BOX    = 1 << 0;
        /// The content size, in device pixels.
        const CONTENT_SIZE  = 1 << 1;
        /// The scroll offset of this node's scrollable area.
        const SCROLL_OFFSET = 1 << 2;
        /// The visible extent of this node's scrollable area.
        const SCROLLPORT    = 1 << 3;
    }
}

/// What is watched on one node, and the values last delivered for each.
///
/// The default is "nothing watched", which is what an unallocated page of the column reads as — so
/// the overwhelming majority of nodes, which nothing watches, cost nothing at all.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ObservationSlots {
    /// Which measurements are watched.
    pub mask: ObservedMask,
    /// The border box last delivered.
    pub border_box: Rect<DevicePx, Device>,
    /// The content size last delivered.
    pub content_size: Size<DevicePx, Device>,
    /// The scroll offset last delivered.
    pub scroll_offset: Point<DevicePx, Device>,
    /// The scrollport extent last delivered.
    pub scrollport: Size<DevicePx, Device>,
}

impl ObservationSlots {
    /// Nothing watched, and no value delivered.
    pub const NONE: Self = Self {
        mask: ObservedMask::empty(),
        border_box: Rect::new(
            Point::new(DevicePx::ZERO, DevicePx::ZERO),
            Size::new(DevicePx::ZERO, DevicePx::ZERO),
        ),
        content_size: Size::new(DevicePx::ZERO, DevicePx::ZERO),
        scroll_offset: Point::new(DevicePx::ZERO, DevicePx::ZERO),
        scrollport: Size::new(DevicePx::ZERO, DevicePx::ZERO),
    };

    /// Whether anything at all is watched here.
    pub fn is_watched(&self) -> bool {
        !self.mask.is_empty()
    }
}

impl Default for ObservationSlots {
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::{ObservationSlots, ObservedMask};

    #[test]
    fn the_default_watches_nothing() {
        let slots = ObservationSlots::default();
        assert!(!slots.is_watched());
        assert_eq!(slots.mask, ObservedMask::empty());
    }

    #[test]
    fn the_slots_are_plain_copyable_data() {
        // Stated as a test because it is the property that keeps the store shareable: nothing here
        // is a closure, a signal or a reference count.
        fn assert_copy<T: Copy>() {}
        assert_copy::<ObservationSlots>();
    }

    #[test]
    fn a_watched_mask_reports_itself_watched() {
        let mut slots = ObservationSlots::NONE;
        slots.mask = ObservedMask::BORDER_BOX | ObservedMask::SCROLL_OFFSET;
        assert!(slots.is_watched());
        assert!(slots.mask.contains(ObservedMask::BORDER_BOX));
        assert!(!slots.mask.contains(ObservedMask::CONTENT_SIZE));
    }
}
