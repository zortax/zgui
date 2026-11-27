//! Glyphs are rasterised by the compositor at the commit that first names them, and counted.
//!
//! Alone in its binary, because the frame counters are the process's and a test beside it would
//! move them.

mod support;

use zgui_geom::{DevicePx, Size};
use zgui_platform::SurfaceEvent;
use zgui_profile::{Counter, counter};
use zgui_view::{BuildCx, IntoView, View};

const CSS: &str = "root { display: block; width: 100%; height: 100% }
                   text { display: block }";

/// The paragraph the window shows. Its eight distinct characters are eight glyphs, and every one
/// sits on a whole pixel, since the fixed face advances eight pixels per cluster.
const WORDS: &str = "hello world";

/// The distinct characters of [`WORDS`].
const DISTINCT: u64 = 8;

/// The first frame rasterises each distinct glyph of its text once, and a frame that records the
/// same glyphs again rasterises none.
#[test]
fn a_frame_rasterises_only_the_glyphs_no_earlier_commit_named() {
    counter::reset();
    let mut app = support::app_with_text(CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().child(WORDS))
                .into_view()
                .build(cx),
        )
    });
    app.settle(16);
    let first = counter::get(Counter::GlyphsRasterised);
    assert!(
        (1..=DISTINCT).contains(&first),
        "the first frame rasterised {first} glyphs for {DISTINCT} distinct characters"
    );

    // A window too narrow for both words breaks the paragraph into two lines, and records both.
    counter::reset();
    app.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(60.0),
        DevicePx(300.0),
    )));
    app.settle(16);
    assert!(
        counter::get(Counter::GlyphsPlaced) > 0,
        "the resize recorded the paragraph's glyphs again"
    );
    assert_eq!(
        counter::get(Counter::GlyphsRasterised),
        0,
        "every glyph the resize recorded was rasterised by an earlier commit"
    );
}
