//! Scroll regions: the `overflow: auto` fixpoint and the gutter a locked container keeps.

mod support;

use std::path::PathBuf;

use support::{Element, Fixture, Frame, lay_out, measurer, relayout};
use zgui_layout::LayoutStore;
use zgui_layout::scroll_region::gutter;
use zgui_testkit_scene::dump::{TreeDump, golden};
use zgui_testkit_scene::text::Writer;

/// A rendering of a layout, as the shared tree-dump seam wants it.
struct Rendering(String);

impl TreeDump for Rendering {
    fn dump(&self, writer: &mut Writer) {
        for line in self.0.lines() {
            writer.line(line);
        }
    }
}

/// Where the goldens live.
fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens")
        .join(format!("{name}.txt"))
}

/// Every fragment's rectangle, in painting order, as text.
fn fragment_rects(store: &LayoutStore, frame: &Frame) -> String {
    let placements = zgui_scene::Placements::of(&frame.spatial);
    let mut out = String::new();
    let root = store.root().expect("a root");
    for key in zgui_layout::fragment::stacking::paint_order(store, root) {
        for frag in store.fragments_of_box(key) {
            let fragment = store.fragment(*frag).expect("live");
            let border = fragment.on_device(fragment.border_box, &placements);
            out.push_str(&format!(
                "{:?} at=({}, {}) size=({} x {})\n",
                fragment.kind,
                border.origin.x.0,
                border.origin.y.0,
                border.size.width.0,
                border.size.height.0,
            ));
        }
    }
    out
}

/// A page whose content overflows the viewport, so its root really does scroll.
fn page(overflow: &str) -> Fixture {
    let rows: Vec<Element> = (0..30)
        .map(|_| Element::new("row").text("alpha bravo delta gamma kappa sigma omega"))
        .collect();
    Fixture::new(
        Element::new("root").children(rows),
        &format!(
            "root {{ display: block; height: 300px; overflow: {overflow} }}
             row {{ display: block }}"
        ),
    )
}

#[test]
fn an_auto_container_reserves_a_gutter_exactly_when_its_content_overflows() {
    // `auto` has no answer until the box has been laid out, so it enters layout reserving nothing
    // and is revised. A container whose content fits must not reserve anything, or every panel in
    // a document loses a scrollbar's width for nothing.
    let fits = Fixture::new(
        Element::new("root").children(vec![Element::new("row")]),
        "root { display: block; height: 300px; overflow: auto }
         row { display: block; height: 20px }",
    );
    let mut store = fits.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 300.0);
    let root = store.root().expect("a root");
    assert_eq!(store.auto_scroll(root), (false, false));
    assert_eq!(
        store
            .layout_of(root)
            .expect("laid out")
            .scrollbar_size
            .width
            .0,
        0.0
    );

    let overflows = page("auto");
    let mut store = overflows.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 300.0);
    let root = store.root().expect("a root");
    assert_eq!(
        store.auto_scroll(root),
        (false, true),
        "the block axis overflowed and only that one"
    );
    assert!(
        store
            .layout_of(root)
            .expect("laid out")
            .scrollbar_size
            .width
            .0
            > 0.0,
        "a vertical scrollbar takes width off the content"
    );
}

#[test]
fn scroll_lock_gutter() {
    // A modal stops the page behind it scrolling. Doing that with `overflow: hidden` alone takes
    // the scrollbar's width back, every line re-wraps wider, and the page jumps sideways behind
    // the modal. Locking the container keeps the gutter, so the fragment rectangles are the same
    // bytes with and without the lock — which is the whole claim, and it is checked as bytes.
    let scrolling = page("auto");
    let mut store = scrolling.box_tree();
    let mut content = measurer();
    let mut frame = lay_out(&mut store, &mut content, 400.0, 300.0);
    let before = fragment_rects(&store, &frame);
    golden::assert_tree(
        &golden_path("scroll-lock-gutter"),
        &Rendering(before.clone()),
    );

    // The same document with the page locked and the style changed to stop it scrolling. The
    // gutter the page was reserving is what the lock holds, and it is read from the layout that
    // reserved it rather than from the one about to be computed.
    let root = store.root().expect("a root");
    let axes = gutter::reserved(&store, root);
    assert_eq!(axes, (false, true), "the page was scrolling vertically");

    let locked = page("hidden");
    let mut locked_store = locked.box_tree();
    let locked_root = locked_store.root().expect("a root");
    gutter::lock_axes(&mut locked_store, locked_root, axes);
    assert!(gutter::is_locked(&locked_store, locked_root));
    let mut content = measurer();
    relayout(&mut frame, &mut locked_store, &mut content, 400.0, 300.0);

    assert_eq!(
        fragment_rects(&locked_store, &frame),
        before,
        "locking the page must not move a single fragment"
    );
}

#[test]
fn unlocking_gives_the_gutter_back() {
    let scrolling = page("auto");
    let mut store = scrolling.box_tree();
    let mut content = measurer();
    let frame = lay_out(&mut store, &mut content, 400.0, 300.0);
    let root = store.root().expect("a root");
    let scrolling_width = fragment_rects(&store, &frame);

    gutter::lock(&mut store, root);
    gutter::unlock(&mut store, root);
    assert!(!gutter::is_locked(&store, root));
    let frame = lay_out(&mut store, &mut content, 400.0, 300.0);
    assert_eq!(fragment_rects(&store, &frame), scrolling_width);
}

#[test]
fn a_padded_bordered_scroller_puts_its_bar_against_the_border() {
    let rows: Vec<Element> = (0..30)
        .map(|_| Element::new("row").text("alpha bravo delta"))
        .collect();
    let fixture = Fixture::new(
        Element::new("root").children(vec![Element::new("column").children(rows)]),
        "root { display: block }
         column { display: block; height: 200px; overflow-y: auto; padding: 0 16px;
                  border: 1px solid red }
         row { display: block }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 300.0);

    let column = fixture
        .document
        .store()
        .core(fixture.root)
        .first_child()
        .expect("the scroller");
    let list = store.boxes_of(fixture.document.store().key_of(column))[0];
    let frags = store.fragments_of_box(list).to_vec();
    let fragment = |kind| {
        frags
            .iter()
            .filter_map(|frag| store.fragment(*frag))
            .find(|fragment| fragment.kind == kind)
            .cloned()
            .expect("the fragment")
    };
    let own = fragment(zgui_layout::FragmentKind::Box);
    let part = |part| {
        fragment(zgui_layout::FragmentKind::Scrollbar {
            axis: zgui_layout::Axis::Vertical,
            part,
        })
        .border_box
    };
    let track = part(zgui_layout::fragment::ScrollbarPart::Track);
    let thumb = part(zgui_layout::fragment::ScrollbarPart::Thumb);

    assert_eq!(
        track.right().0,
        own.border_box.right().0 - 1.0,
        "the track touches the border"
    );
    assert_eq!(track.top(), own.padding_box().top());
    assert_eq!(track.bottom(), own.padding_box().bottom());
    assert!(
        (track.left().0 - (own.content_box().right().0 + 16.0)).abs() < 1e-3,
        "the right padding stays between the content and the bar"
    );
    assert_eq!(thumb.left(), track.left());
    assert_eq!(thumb.top(), track.top());
}

#[test]
fn a_box_is_found_in_its_scrollers_content_space_from_layout_alone() {
    let rows: Vec<Element> = (0..30)
        .map(|_| Element::new("row").classes(&["r"]))
        .collect();
    let fixture = Fixture::new(
        Element::new("root").children(vec![
            Element::new("column").children(vec![Element::new("box").children(rows)]),
        ]),
        "root { display: block }
         column { display: block; height: 200px; overflow-y: auto; padding: 10px;
                  border: 2px solid red }
         box { display: block; margin-top: 7px }
         row { display: block; height: 20px }",
    );
    let mut store = fixture.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 300.0);

    let document = fixture.document.store();
    let column = document
        .core(fixture.root)
        .first_child()
        .expect("the scroller");
    let inner = document.core(column).first_child().expect("the wrapper");
    let mut row = document.core(inner).first_child().expect("a row");
    for _ in 0..5 {
        row = document.core(row).next_sibling().expect("a later row");
    }
    let container = store.boxes_of(document.key_of(column))[0];
    let target = store.boxes_of(document.key_of(row))[0];

    let (rect, port) = zgui_layout::scroll_region::into_view::target_in(&store, container, target)
        .expect("the row is below the scroller");
    assert_eq!(
        rect.origin.y.0,
        7.0 + 5.0 * 20.0,
        "the wrapper's margin and five rows"
    );
    assert_eq!(
        rect.origin.x.0, 0.0,
        "measured from the content box, inside the padding"
    );
    assert_eq!(rect.size.height.0, 20.0);
    assert_eq!(
        port.height.0,
        200.0 - 2.0 * 10.0 - 2.0 * 2.0,
        "the scrollport is the content box of a border-box sized scroller"
    );
    assert!(
        zgui_layout::scroll_region::into_view::target_in(&store, target, container).is_none(),
        "the scroller is not below the row"
    );
}
