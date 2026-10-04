//! Whether text on the pixel grid is drawn with per-channel coverage.
//!
//! Per-channel coverage leaves a coloured fringe along each stroke of dark text on a light page:
//! the three channels of an edge pixel are covered by different amounts. Whole-pixel coverage
//! leaves grey. So the fringe is what tells the two apart in the picture.

mod desktop;
mod device;
mod painted;

use zgui::view;
use zgui::view::AnyView;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff; color: #000000; font-family: sans-serif }
                     .page { padding: 40px; gap: 40px; align-items: flex-start }
                     .scaled { transform: scale(1.5) }";

/// How many pixels in the smallest box saying `text` have channels more than 30 levels apart.
fn fringed(stage: &Stage, text: &str) -> usize {
    let rect = stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text == text && node.area() > 0.0)
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect)
        .unwrap_or_else(|| panic!("nothing placed says {text:?}"));
    stage
        .colours_in(rect)
        .iter()
        .filter(|colour| {
            let high = colour.0.max(colour.1).max(colour.2);
            let low = colour.0.min(colour.1).min(colour.2);
            high - low > 30
        })
        .count()
}

#[test]
fn text_on_the_pixel_grid_has_per_channel_coverage_and_scaled_text_does_not() {
    if !device::subpixel_text() {
        eprintln!("skipped: the device draws no per-channel coverage");
        return;
    }
    let Some(stage) = Stage::open(SHEET, || {
        AnyView::new(view! {
            column(class = "page") {
                text {"Upright handgloves"}
                text(class = "scaled") {"Scaled handgloves"}
            }
        })
    }) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };

    let upright = fringed(&stage, "Upright handgloves");
    assert!(
        upright > 40,
        "text on the pixel grid of an opaque page is drawn with per-channel coverage: {upright} \
         fringed pixels"
    );
    let scaled = fringed(&stage, "Scaled handgloves");
    assert_eq!(
        scaled, 0,
        "scaled text is resampled, so it is drawn with whole-pixel coverage"
    );
}
