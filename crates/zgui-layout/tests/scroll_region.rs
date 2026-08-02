//! Scroll regions: the `overflow: auto` fixpoint and the gutter a locked container keeps.

mod support;

use std::path::PathBuf;

use support::{Element, Fixture, lay_out, measurer, relayout};
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
fn fragment_rects(store: &LayoutStore) -> String {
    let mut out = String::new();
    let root = store.root().expect("a root");
    for key in zgui_layout::fragment::stacking::paint_order(store, root) {
        for frag in store.fragments_of_box(key) {
            let fragment = store.fragment(*frag).expect("live");
            out.push_str(&format!(
                "{:?} at=({}, {}) size=({} x {})\n",
                fragment.kind,
                fragment.border_box.origin.x.0,
                fragment.border_box.origin.y.0,
                fragment.border_box.size.width.0,
                fragment.border_box.size.height.0,
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
    let before = fragment_rects(&store);
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
        fragment_rects(&locked_store),
        before,
        "locking the page must not move a single fragment"
    );
}

#[test]
fn unlocking_gives_the_gutter_back() {
    let scrolling = page("auto");
    let mut store = scrolling.box_tree();
    let mut content = measurer();
    lay_out(&mut store, &mut content, 400.0, 300.0);
    let root = store.root().expect("a root");
    let scrolling_width = fragment_rects(&store);

    gutter::lock(&mut store, root);
    gutter::unlock(&mut store, root);
    assert!(!gutter::is_locked(&store, root));
    lay_out(&mut store, &mut content, 400.0, 300.0);
    assert_eq!(fragment_rects(&store), scrolling_width);
}
