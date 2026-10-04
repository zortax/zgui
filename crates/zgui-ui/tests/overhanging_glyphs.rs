//! Glyphs on a line tighter than their face.
//!
//! A line height tighter than the face puts the glyphs above and below the line box, half the
//! negative leading on each side. They have to be drawn there.

mod desktop;
mod device;
mod painted;

use zgui::geom::{DevicePx, Point, Rect, Size};
use zgui::view;
use zgui::view::AnyView;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 80px 40px; align-items: flex-start }
                     .pair { flex-direction: row; gap: 40px; align-items: flex-start }
                     .tight { font-size: 40px; line-height: 4px }
                     .loose { font-size: 40px; line-height: 80px }";

/// How far below the middle of the smallest box saying `text` the bottom of its ink is.
///
/// The ink is capitals, so its bottom is the baseline.
fn baseline_below_middle(stage: &Stage, text: &str) -> f32 {
    let census = stage.census();
    let line = census
        .nodes
        .iter()
        .filter(|node| node.text == text && node.area() > 0.0)
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect)
        .unwrap_or_else(|| panic!("nothing placed says {text:?}"));
    let middle = line.origin.y.0 + line.size.height.0 / 2.0;
    let band = Rect::new(
        Point::new(line.origin.x, DevicePx(middle - 60.0)),
        Size::new(line.size.width, DevicePx(120.0)),
    );
    let width = band.size.width.0 as usize;
    let colours = stage.composed_colours_in(band);
    let bottom = colours
        .chunks(width)
        .rposition(|row| row.iter().any(|colour| colour.0 < 100))
        .unwrap_or_else(|| panic!("{text:?} drew nothing near its line"));
    bottom as f32 + 1.0 - 60.0
}

#[test]
fn a_tight_line_puts_its_baseline_where_a_loose_one_does() {
    let Some(stage) = Stage::open(SHEET, || {
        AnyView::new(view! {
            column(class = "page") {
                row(class = "pair") {
                    text(class = "tight") {"HH"}
                    text(class = "loose") {"HHH"}
                }
            }
        })
    }) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let tight = baseline_below_middle(&stage, "HH");
    let loose = baseline_below_middle(&stage, "HHH");
    assert!(
        (tight - loose).abs() <= 1.0,
        "the baseline is {tight} below the middle of a tight line and {loose} below a loose one"
    );
}
