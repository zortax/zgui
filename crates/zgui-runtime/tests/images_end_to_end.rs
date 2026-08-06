//! An `image` element, from `src` to a laid-out picture, through the real frame loop.
//!
//! This is the wiring test the loader exists for: the view writes `src`, the attribute hook
//! queues it, a settle kicks the decode onto the blocking pool, the completion wakes the loop,
//! the next settle files the intrinsic and attaches the texels, and layout gives the box the
//! picture's natural size. Nothing here reaches into the loader — every assertion is against
//! what a user of the framework can observe: the fragment tree and the budget report.

mod support;

use std::time::{Duration, Instant};

use zgui_platform_headless::Harness;
use zgui_runtime::Runtime;
use zgui_runtime::budget::CacheId;
use zgui_view::{BuildCx, IntoView, View};

/// The fixture picture: two by two, opaque red, and — the part layout shows — 2×2 CSS pixels.
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/red2x2.png"
);

/// No author sizing on the image at all: its size *is* the assertion. The element keeps its
/// vocabulary default — an inline block — which is what shrink-fits to the natural size.
const CSS: &str = "root { display: block; width: 400px; height: 300px }";

/// A window holding one image element pointed at `src`.
///
/// With a real (deterministic) text engine behind it, and that is load-bearing: an inline-block
/// image is placed by the line it sits on, and a window with no shaper breaks no lines.
fn window(src: String) -> Harness<Runtime> {
    support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::r#box()
                .class("root")
                .child(zgui_elements::image().src(Some(src.clone())))
                .into_view()
                .build(cx),
        )
    })
}

/// The border box of the image element's fragment, if it has one yet.
fn image_box(window: &zgui_runtime::Window) -> Option<(f32, f32)> {
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    for key in layout.keys() {
        let node = layout.node(key);
        let Some(source) = node.source else {
            continue;
        };
        let Some(index) = document.store().index_of(source) else {
            continue;
        };
        if document.store().core(index).local_name().as_str() != "image" {
            continue;
        }
        let fragment = layout.fragments_of_box(key).first()?;
        let fragment = layout.fragment(*fragment)?;
        return Some((
            fragment.border_box.size.width.0,
            fragment.border_box.size.height.0,
        ));
    }
    None
}

/// Pumps the loop until the decode has landed and layout shows it, or fails loudly.
///
/// The decode runs on a real thread, so the loop legitimately parks while it is in flight; what
/// ends the wait is the completion's wake. The polling sleep is the test waiting on a thread it
/// does not own, not the framework needing to be polled.
fn settle_until_shown(app: &mut Harness<Runtime>) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        app.settle(64);
        let shown = {
            let window = &app.app_mut().windows_mut()[0];
            image_box(window) == Some((2.0, 2.0))
        };
        if shown {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the decode never reached layout; the box is at {:?}",
            image_box(&app.app_mut().windows_mut()[0])
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_file_src_becomes_a_naturally_sized_box_and_survives_forget() {
    let mut app = window(FIXTURE.to_owned());
    app.settle(8);

    // Before the decode lands the box is replaced, unsized, and drawn as nothing — the page is
    // already in its final shape for CSS-sized images, and for auto ones it is not wrong, only
    // not final. What matters is that the frame loop is parked, not spinning, while it waits;
    // `settle` returning is itself that assertion.

    settle_until_shown(&mut app);
    {
        let window = &mut app.app_mut().windows_mut()[0];
        let images = window.budget_report().line(CacheId::DecodedImages).report;
        assert!(images.resident > 0, "the loader holds what it decoded");
        assert_eq!(
            images.pinned, images.resident,
            "a picture that is on the screen is pinned, all of it"
        );

        // The whole point of the loader over the old embedder contract: a forgotten window gets
        // its pictures back by itself, because it kept their names.
        window.forget_caches();
        assert_eq!(
            window.budget_report().line(CacheId::DecodedImages).report.resident,
            0,
            "forget dropped the texels"
        );
    }
    settle_until_shown(&mut app);
    let window = &mut app.app_mut().windows_mut()[0];
    assert!(
        window.budget_report().line(CacheId::DecodedImages).report.resident > 0,
        "the re-decode restored what forget dropped, from the source the loader kept"
    );
}

#[test]
fn in_memory_bytes_travel_the_same_wire_as_a_path() {
    let bytes = zgui_image::ImageBytes::new(std::fs::read(FIXTURE).expect("the fixture exists"));
    let mut app = window(bytes.url());
    app.settle(8);
    settle_until_shown(&mut app);
}
