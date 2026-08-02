//! Where a fragment ends up: the ancestor chain, the clips, the scroll offset and the paint order.

use zgui_geom::{DevicePx, Point};
use zgui_layout::FragmentFlags;
use zgui_layout::fragment::diff::Everything;

use crate::probe::{box_named, own_fragment};
use crate::support::{Element, Fixture, fragments, lay_out, measurer};

#[test]
fn a_fragment_sits_where_the_whole_ancestor_chain_puts_it() {
    // Three nested boxes each contributing a border and a padding: the innermost fragment's
    // absolute origin is the sum of every inset above it, and nothing but this composition
    // produces that number.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("outer").children(vec![Element::new("inner")]),
        ]),
        "root { display: block; width: 400px; padding: 10px }
         outer { display: block; border: 2px solid black; padding: 5px }
         inner { display: block; height: 20px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 400.0);

    let inner = box_named(&store, &fixture, "inner");
    let fragment = own_fragment(&store, inner);
    assert_eq!(
        fragment.border_box.origin,
        Point::new(DevicePx(17.0), DevicePx(17.0)),
        "10 of the root's padding, 2 of the outer's border and 5 of its padding"
    );
}

#[test]
fn a_scroll_container_clips_its_descendants_and_nothing_above_it() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("port").children(vec![Element::new("row")]),
        ]),
        "root { display: block; width: 300px }
         port { display: block; height: 100px; overflow: hidden }
         row { display: block; height: 400px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 300.0, 300.0);

    let port = box_named(&store, &fixture, "port");
    let row = box_named(&store, &fixture, "row");
    let port_fragment = own_fragment(&store, port);
    let row_fragment = own_fragment(&store, row);

    assert!(port_fragment.flags.contains(FragmentFlags::CLIPS_CHILDREN));
    assert_eq!(
        port_fragment.clip,
        zgui_scene::ClipId::ROOT,
        "a box is not clipped by itself"
    );
    assert_ne!(row_fragment.clip, zgui_scene::ClipId::ROOT);
    let resolved = frame.clips.resolve(row_fragment.clip);
    assert_eq!(
        resolved.aabb[3], 100.0,
        "clipped to the scrollport's height"
    );
}

#[test]
fn a_scrolled_container_moves_its_contents_and_leaves_itself_alone() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("port").children(vec![Element::new("row")]),
        ]),
        "root { display: block; width: 300px }
         port { display: block; height: 100px; overflow: scroll }
         row { display: block; height: 400px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 300.0);

    let port = box_named(&store, &fixture, "port");
    let row = box_named(&store, &fixture, "row");
    let before = own_fragment(&store, row).border_box.origin.y.0;

    let scrolled = frame.scroll.scroll_to(
        &store,
        store
            .node(port)
            .source
            .expect("the scrollport is an element"),
        Point::new(DevicePx(0.0), DevicePx(60.0)),
    );
    assert_eq!(scrolled.y, DevicePx(60.0), "the content is long enough");
    let root = store.root().expect("a root");
    fragments(&mut frame, &mut store, root, &mut Everything);

    assert_eq!(
        own_fragment(&store, row).border_box.origin.y.0,
        before - 60.0,
        "the row moved up by the scroll offset"
    );
    assert_eq!(
        own_fragment(&store, port).border_box.origin.y.0,
        0.0,
        "the container itself did not move"
    );
}

/// A sticky box with nothing scrollable above it sticks against the window, and the fold that
/// decides whether a subtree can be moved as a block has to know it.
///
/// It is the case where the region a sticky shift is measured against has no box to be named after,
/// and a fold that took the absence of a name for the absence of stickiness would report the whole
/// document as moving in one piece.
#[test]
fn a_sticky_box_with_nothing_scrolling_above_it_is_not_rigid() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("head"), Element::new("body")]),
        "root { display: block; width: 300px }
         head { display: block; height: 20px; position: sticky; top: 0 }
         body { display: block; height: 600px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let _frame = lay_out(&mut store, &mut content, 300.0, 300.0);

    let head = box_named(&store, &fixture, "head");
    let root = store.root().expect("a root");
    assert!(
        own_fragment(&store, head)
            .flags
            .contains(FragmentFlags::IS_STICKY)
    );
    assert!(
        !own_fragment(&store, root).subtree_rigid,
        "a subtree holding a sticky box does not move by one vector"
    );
}

#[test]
fn a_sticky_header_stops_at_the_top_of_its_scrollport() {
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("port").children(vec![
            Element::new("spacer"),
            Element::new("head"),
            Element::new("body"),
        ])]),
        "root { display: block; width: 300px }
         port { display: block; height: 200px; overflow: scroll }
         spacer { display: block; height: 100px }
         head { display: block; height: 20px; position: sticky; top: 0 }
         body { display: block; height: 600px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 300.0, 300.0);

    let port = box_named(&store, &fixture, "port");
    let head = box_named(&store, &fixture, "head");
    assert!(
        own_fragment(&store, head)
            .flags
            .contains(FragmentFlags::IS_STICKY)
    );
    assert_eq!(own_fragment(&store, head).border_box.origin.y.0, 100.0);

    // Scrolled past where the header sat, it stays at the top of the port instead of leaving it.
    frame.scroll.scroll_to(
        &store,
        store
            .node(port)
            .source
            .expect("the scrollport is an element"),
        Point::new(DevicePx(0.0), DevicePx(160.0)),
    );
    let root = store.root().expect("a root");
    fragments(&mut frame, &mut store, root, &mut Everything);
    assert_eq!(
        own_fragment(&store, head).border_box.origin.y.0,
        0.0,
        "the header is held at the scrollport's top edge"
    );
}

#[test]
fn a_positive_z_index_paints_after_a_later_sibling() {
    // Document order says `under` then `over`; `z-index` says otherwise, and painting order is
    // what decides which one a click lands on.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("over"), Element::new("under")]),
        "root { display: block; width: 200px; position: relative }
         over { display: block; height: 50px; position: relative; z-index: 5 }
         under { display: block; height: 50px; position: relative; z-index: 1 }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 200.0, 200.0);

    let root = store.root().expect("a root");
    let order = zgui_layout::fragment::stacking::paint_order(&store, root);
    let over = box_named(&store, &fixture, "over");
    let under = box_named(&store, &fixture, "under");
    let at = |key| order.iter().position(|held| *held == key).expect("painted");
    assert!(
        at(under) < at(over),
        "the lower index is painted first however the document was written"
    );
    assert!(
        own_fragment(&store, over)
            .flags
            .contains(FragmentFlags::IS_STACKING_CONTEXT)
    );
}

#[test]
fn a_fixed_box_at_half_the_viewport_pulled_back_by_half_itself_is_centred() {
    // How every modal dialog is centred, and the only arrangement that centres one whose size is
    // not known in advance: half the viewport puts its leading corner at the centre, and a
    // percentage translate — resolved against the element's own border box — pulls it back by half
    // of itself. Get either half wrong and the panel hangs below and right of centre by half its
    // own size, which is what a dialog centred by position alone does.
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("panel")]),
        "root { display: block; width: 800px; height: 600px }
         panel {
             display: block;
             position: fixed;
             left: 50%;
             top: 50%;
             width: 400px;
             height: 200px;
             transform: translate(-50%, -50%);
         }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 800.0, 600.0);

    let panel = box_named(&store, &fixture, "panel");
    let fragment = own_fragment(&store, panel);
    // A transform does not move the layout box, so the box itself is at the centre; what centres
    // the panel is the matrix the fragment carries.
    assert_eq!(
        fragment.border_box.origin,
        Point::new(DevicePx(400.0), DevicePx(300.0)),
        "half the viewport, which is where the leading corner belongs before the pull-back"
    );
    let id = fragment
        .transform
        .expect("a translated panel carries a matrix");
    let matrix = frame
        .spatial
        .resolve(id)
        .expect("the name resolves to a matrix");
    assert_eq!(
        (matrix.columns[3][0], matrix.columns[3][1]),
        (-200.0, -100.0),
        "the pull-back is half the panel, so the painted centre lands on the viewport's"
    );
}

#[test]
fn a_fixed_box_covers_the_viewport_and_not_the_document_below_it() {
    // A scrim covers the window, whatever is scrolled behind it. Both ways of asking for that are
    // checked here because they resolve differently: `right`/`bottom` name the containing block's
    // far edges, while `width: 100%` resolves a percentage against it. If a fixed box's containing
    // block were the document rather than the viewport, the second would run past the window and
    // the first would not — and a scrim that covers the scrollport instead of the window leaves an
    // uncovered strip down the right and along the bottom.
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("tall"),
            Element::new("insets"),
            Element::new("percent"),
        ]),
        "root { display: block; width: 800px }
         tall { display: block; height: 4000px }
         insets { position: fixed; left: 0; top: 0; right: 0; bottom: 0 }
         percent { position: fixed; left: 0; top: 0; width: 100%; height: 100% }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 800.0, 600.0);

    for name in ["insets", "percent"] {
        let fragment = own_fragment(&store, box_named(&store, &fixture, name));
        assert_eq!(
            (
                fragment.border_box.size.width.0,
                fragment.border_box.size.height.0
            ),
            (800.0, 600.0),
            "`{name}` has to cover the window, not the 4000px document behind it"
        );
    }
}
