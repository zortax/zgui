//! Scrollable regions: what scrolls, how far it can scroll, and what it reserves.
//!
//! Layout owns the *region* — which boxes scroll, how large their content is, where the scrollport
//! is and what gutter is reserved for a scrollbar. It does not own the *offset*: how far a region
//! has been scrolled is state that changes many times a second and must never re-enter layout, so
//! it is supplied to the fragment pass from outside and composed in there.

pub mod auto;
pub mod bar;
pub mod gutter;

use rustc_hash::FxHashMap;
use zgui_css::ComputedStyle;
use zgui_css::values::size::OverflowValue;
use zgui_dom::NodeKey;
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};

use crate::tree::store::LayoutStore;

/// Whether a box scrolls its content rather than letting it spill or cutting it off.
///
/// `auto` counts: whether it *shows* a scrollbar depends on the content, but the box is a scroll
/// container either way, and the clip and the scroll frame it establishes do not appear and
/// disappear with the content.
pub fn is_scroll_container(style: &ComputedStyle) -> bool {
    let box_ = style.get_box();
    scrolls(box_.overflow_x) || scrolls(box_.overflow_y)
}

/// Whether one axis's value makes a box scrollable.
fn scrolls(value: OverflowValue) -> bool {
    matches!(value, OverflowValue::Auto | OverflowValue::Scroll)
}

/// One scrollable region, as layout resolved it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollRegion {
    /// The visible rectangle, in the same local space the box's fragments are in.
    pub scrollport: Rect<DevicePx, Device>,
    /// How far the content reaches inside it.
    pub content: Size<DevicePx, Device>,
}

impl ScrollRegion {
    /// The largest offset the region can be scrolled to, which is never negative.
    ///
    /// Content smaller than its scrollport cannot be scrolled at all, and the limit is zero rather
    /// than a negative number so that clamping an offset to it is one comparison.
    pub fn limit(&self) -> Point<DevicePx, Device> {
        Point::new(
            DevicePx((self.content.width.0 - self.scrollport.size.width.0).max(0.0)),
            DevicePx((self.content.height.0 - self.scrollport.size.height.0).max(0.0)),
        )
    }
}

/// The region one box scrolls, or nothing when it is not a scroll container.
///
/// Read from the box's own resolved layout, so it is available as soon as layout has run and needs
/// no separate pass.
pub fn region_of(store: &LayoutStore, key: BoxKey) -> Option<ScrollRegion> {
    let node = store.get(key)?;
    if !is_scroll_container(&node.style) {
        return None;
    }
    let layout = store.layout_of(key)?;
    let scrollport = layout.content_box();
    Some(ScrollRegion {
        scrollport: Rect::new(Point::new(DevicePx(0.0), DevicePx(0.0)), scrollport.size),
        content: layout.content_size,
    })
}

/// The region one element scrolls, or nothing when it is not a scroll container.
///
/// An element generates its boxes; the first of them is the one an offset is measured against,
/// which is the same box every other consumer of an element's geometry reads.
pub fn region_of_element(store: &LayoutStore, element: NodeKey) -> Option<ScrollRegion> {
    region_of(store, *store.boxes_of(element).first()?)
}

/// Where each scroll container is scrolled to, filed under the **element** that scrolls.
///
/// Held outside the layout store because a scroll offset is not a layout input: writing one marks
/// the container's subtree for repositioning and repainting and nothing else, and the fragment pass
/// adds it to the composed positions. A container with no entry is at the origin, which is every
/// container in a document nobody has scrolled.
///
/// # Why the element and not the box
///
/// A box is not a stable name for anything. Adding or removing a box anywhere in the document
/// rebuilds the whole box tree, and every box in it is issued a new key — so an offset filed under
/// a box is an offset that survives exactly until the next tooltip appears, the next menu opens or
/// the next `:hover` rule reveals a control. The container is still there, still scrolled, and its
/// offset is now filed under a name nothing will ever ask about again: the page silently jumps back
/// to the top, and no assertion about clamping, marking or composition can see it happen.
///
/// The element survives all of that, because it is what the document is made of and the boxes are
/// what it generates. So the element is where an offset lives.
#[derive(Clone, Debug, Default)]
pub struct ScrollOffsets {
    /// The elements that have been scrolled away from the origin.
    offsets: FxHashMap<NodeKey, Point<DevicePx, Device>>,
}

impl ScrollOffsets {
    /// No container is scrolled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether nothing has been scrolled, which is the common case and is worth one test.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Every element that has been scrolled away from the origin, and how far.
    ///
    /// In no particular order, because the map is not ordered and nothing that reads this cares:
    /// what it exists for is the sweep that re-clamps held offsets after the extents they are
    /// clamped against have moved, and each container is answered independently of the rest.
    pub fn iter(&self) -> impl Iterator<Item = (NodeKey, Point<DevicePx, Device>)> + '_ {
        self.offsets.iter().map(|(element, at)| (*element, *at))
    }

    /// How far one element is scrolled.
    pub fn of(&self, element: NodeKey) -> Point<DevicePx, Device> {
        self.offsets
            .get(&element)
            .copied()
            .unwrap_or(Point::new(DevicePx(0.0), DevicePx(0.0)))
    }

    /// How far the element that generated one box is scrolled.
    ///
    /// What the fragment pass asks, because it walks boxes. A box with no element behind it — an
    /// anonymous wrapper — is never a scroll container, so it is at the origin.
    pub fn of_box(&self, store: &LayoutStore, key: BoxKey) -> Point<DevicePx, Device> {
        store
            .get(key)
            .and_then(|node| node.source)
            .map_or(Point::new(DevicePx(0.0), DevicePx(0.0)), |element| {
                self.of(element)
            })
    }

    /// Scrolls one element to `offset`, clamped to what its content allows.
    ///
    /// Clamped here rather than by the caller because the limit is layout's answer and nobody
    /// else's: a caller that clamped for itself would be reading a content size it does not own.
    pub fn scroll_to(
        &mut self,
        store: &LayoutStore,
        element: NodeKey,
        offset: Point<DevicePx, Device>,
    ) -> Point<DevicePx, Device> {
        let limit = region_of_element(store, element)
            .map(|region| region.limit())
            .unwrap_or(Point::new(DevicePx(0.0), DevicePx(0.0)));
        let clamped = Point::new(
            DevicePx(offset.x.0.clamp(0.0, limit.x.0)),
            DevicePx(offset.y.0.clamp(0.0, limit.y.0)),
        );
        self.offsets.insert(element, clamped);
        clamped
    }

    /// Puts one element's offset at `offset`, whatever its content allows.
    ///
    /// [`ScrollOffsets::scroll_to`] is what a scroll is, and it clamps. This is for the *composed*
    /// value the fragment pass reads, which carries a displacement past the end that the content
    /// extent does not allow: content dragged past its edge follows the gesture and springs back,
    /// and the scrollbar, the reported offset and the observation a virtualiser reads must all keep
    /// answering with the clamped position while it does.
    pub fn place(&mut self, element: NodeKey, offset: Point<DevicePx, Device>) {
        self.offsets.insert(element, offset);
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};

    use super::ScrollRegion;

    #[test]
    fn content_that_fits_cannot_be_scrolled() {
        let region = ScrollRegion {
            scrollport: Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(100.0), DevicePx(100.0)),
            ),
            content: Size::new(DevicePx(40.0), DevicePx(40.0)),
        };
        assert_eq!(region.limit(), Point::new(DevicePx(0.0), DevicePx(0.0)));
    }

    #[test]
    fn the_limit_is_what_the_content_exceeds_the_port_by() {
        let region: ScrollRegion = ScrollRegion {
            scrollport: Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::<DevicePx, Device>::new(DevicePx(100.0), DevicePx(50.0)),
            ),
            content: Size::new(DevicePx(100.0), DevicePx(400.0)),
        };
        assert_eq!(region.limit(), Point::new(DevicePx(0.0), DevicePx(350.0)));
    }
}
