//! Scene transcripts: what the paint stage actually produced, as text a review can read.
//!
//! A transcript is the regression artifact for this stage because it is *diffable*. A golden that
//! is only a count — "twelve primitives" — passes while every one of them changes; a transcript
//! fails on the field that moved and names it.

mod support;

use std::path::PathBuf;

use support::{Element, Harness};
use zgui_testkit_scene::dump::golden;

/// The golden for one named scene.
fn path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/goldens/scene")
        .join(format!("{name}.txt"))
}

/// Renders `harness`'s finished scene and compares it against the golden called `name`.
fn assert_scene(name: &str, harness: &Harness) {
    let transcript = zgui_testkit_scene::transcript::of(&harness.scene, &harness.damage);
    golden::assert_matches(&path(name), transcript.as_str());
}

#[test]
fn card_with_shadow() {
    // A card carrying every box decoration at once, so that the order of the four — shadow behind
    // the background, background behind the border, inset shadow over the background, outline after
    // the descendants — is what the golden pins.
    let mut harness = Harness::sized(
        Element::new("root").children(vec![
            Element::new("card").children(vec![Element::new("row")]),
        ]),
        "root { display: block; width: 256px; height: 256px; background: #fff }
         card {
             display: block;
             margin: 16px;
             height: 96px;
             background: #eeeeee;
             border: 2px dashed #336699;
             border-radius: 20px / 10px;
             box-shadow: 0 4px 8px 2px rgba(0, 0, 0, 0.25), inset 0 1px 0 0 #ffffff;
             outline: 2px solid #0066ff;
         }
         row { display: block; height: 24px; background: #333333 }",
        256.0,
        256.0,
    );
    harness.paint_everything();
    assert_scene("card_with_shadow", &harness);
}

#[test]
fn overlay_layer_order() {
    // A toast mounted *first* in document order and given a positive stacking index has to draw
    // over the page mounted after it. The assertion is on the transcript's draw orders, which is
    // the only place the answer is visible before a renderer exists.
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("toast"), Element::new("page")]),
        "root { display: block; width: 200px; height: 120px }
         toast {
             display: block;
             position: absolute;
             top: 8px;
             left: 8px;
             width: 120px;
             height: 32px;
             z-index: 10;
             background: #222222;
         }
         page { display: block; height: 120px; background: #dddddd }",
        200.0,
        120.0,
    );
    harness.paint_everything();

    let toast = harness.box_of("toast");
    let page = harness.box_of("page");
    let toast_ink = harness
        .store
        .fragment(harness.fragment_of("toast"))
        .expect("a fragment")
        .border_box;
    let page_ink = harness
        .store
        .fragment(harness.fragment_of("page"))
        .expect("a fragment")
        .border_box;
    assert!(
        toast_ink.intersects(page_ink),
        "the two have to overlap, or the ordering question does not arise"
    );
    let _ = (toast, page);

    let quads = &harness.scene.primitives.quads;
    let toast_quad = quads
        .iter()
        .find(|quad| quad.bounds[3] == 32.0)
        .expect("the toast's quad");
    let page_quad = quads
        .iter()
        .find(|quad| quad.bounds[3] == 120.0)
        .expect("the page's quad");
    assert!(
        toast_quad.order > page_quad.order,
        "the toast at {} has to sort over the page at {}",
        toast_quad.order,
        page_quad.order
    );

    assert_scene("overlay_layer_order", &harness);
}
