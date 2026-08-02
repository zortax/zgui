//! What a scroll tells whoever asked to hear about it.

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Scale};
use zgui_layout::LayoutStore;
use zgui_layout::scroll_region::region_of_element;
use zgui_vocab::ScrollEvent;

/// One container that moved.
///
/// The offsets are device pixels because that is what the fragment pass composes in; the event a
/// listener receives is in CSS pixels because that is the unit a document is written in. The
/// conversion happens once, here, rather than in each of the places that report a scroll.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scrolled {
    /// The element that scrolled.
    pub container: NodeKey,
    /// Where it was.
    pub from: Point<DevicePx, Device>,
    /// Where it is now.
    pub to: Point<DevicePx, Device>,
}

/// The event describing where `container` now sits, or nothing when it is not a scroll container.
///
/// Read out of the region layout already computed rather than from anything this crate keeps: the
/// content extent and the scrollport are layout's answers, and a second copy of them here would be
/// a copy that disagrees on the frame a container's content changes size.
pub fn event(
    store: &LayoutStore,
    container: NodeKey,
    at: Point<DevicePx, Device>,
    scale: Scale<Css, Device>,
) -> Option<ScrollEvent> {
    let region = region_of_element(store, container)?;
    let to_css = |value: f32| CssPx(value / scale.get());
    Some(ScrollEvent {
        offset: Point::new(to_css(at.x.0), to_css(at.y.0)),
        content_size: zgui_geom::Size::new(
            to_css(region.content.width.0),
            to_css(region.content.height.0),
        ),
        scrollport: zgui_geom::Size::new(
            to_css(region.scrollport.size.width.0),
            to_css(region.scrollport.size.height.0),
        ),
    })
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters};
    use zgui_geom::{Device, DevicePx, Point, Scale};
    use zgui_interned::ElementName;
    use zgui_layout::LayoutStore;

    use super::event;

    #[test]
    fn a_box_that_does_not_scroll_reports_nothing() {
        let document = Document::new();
        let root = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                root
            })
            .expect("not poisoned");
        let _ = root;
        let store = LayoutStore::new(document.store().document());
        let missing = store.root().and_then(|key| store.get(key)?.source);
        assert!(
            missing
                .and_then(|element| event(
                    &store,
                    element,
                    Point::new(DevicePx(0.0), DevicePx(0.0)),
                    Scale::<zgui_geom::Css, Device>::new(1.0),
                ))
                .is_none()
        );
    }
}
