//! What the store has to get right about identity and geometry.

use zgui_arena::DocumentId;
use zgui_css::StyleDraft;
use zgui_geom::{DevicePx, Edges, Point, Size};

use crate::node::box_node::BoxNode;
use crate::node::kind::{BoxKind, FormattingContext};

use super::{BOX_ARENA, FRAGMENT_ARENA, LayoutStore, ResolvedLayout};

#[test]
fn the_box_and_fragment_arenas_are_different_key_spaces() {
    assert_ne!(BOX_ARENA, FRAGMENT_ARENA);
    let store = LayoutStore::new(DocumentId::FIRST);
    assert_eq!(store.box_domain().arena(), BOX_ARENA);
    assert_eq!(store.box_domain().document(), DocumentId::FIRST);
}

#[test]
fn a_removed_box_stops_being_named_by_its_element() {
    let mut store = LayoutStore::new(DocumentId::FIRST);
    let style = StyleDraft::initial().build();
    let key = store.insert(BoxNode::new(
        style,
        BoxKind::Element,
        FormattingContext::Block,
    ));
    assert!(store.contains(key));
    assert_eq!(store.len(), 1);
    assert!(store.remove(key));
    assert!(store.contains(key), "the frame is not over");
    store.recycle();
    assert!(!store.contains(key));
}

#[test]
fn the_content_box_is_the_border_box_less_every_inset() {
    let layout = ResolvedLayout {
        origin: Point::new(DevicePx(10.0), DevicePx(20.0)),
        size: Size::new(DevicePx(100.0), DevicePx(50.0)),
        border: Edges {
            top: DevicePx(1.0),
            right: DevicePx(2.0),
            bottom: DevicePx(3.0),
            left: DevicePx(4.0),
        },
        padding: Edges {
            top: DevicePx(5.0),
            right: DevicePx(6.0),
            bottom: DevicePx(7.0),
            left: DevicePx(8.0),
        },
        scrollbar_size: Size::new(DevicePx(9.0), DevicePx(0.0)),
        ..ResolvedLayout::default()
    };
    let padding_box = layout.padding_box();
    assert_eq!(
        padding_box.origin,
        Point::new(DevicePx(14.0), DevicePx(21.0))
    );
    assert_eq!(padding_box.size, Size::new(DevicePx(94.0), DevicePx(46.0)));
    let content_box = layout.content_box();
    assert_eq!(
        content_box.origin,
        Point::new(DevicePx(22.0), DevicePx(26.0))
    );
    assert_eq!(content_box.size, Size::new(DevicePx(71.0), DevicePx(34.0)));
}
