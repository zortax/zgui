//! Glyphs on a line tighter than their face.
//!
//! A line height tighter than the face puts the glyphs above and below the line box, half the
//! negative leading on each side. They have to be drawn there, and when the line changes, the
//! repaint has to cover the old glyphs and the new ones. A repaint cut to the line box leaves the
//! old descenders behind and cuts the new ones off.
//!
//! Read off the composed target with the real damage set, which is what a surface shows.

mod desktop;
mod device;
mod painted;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use zgui::geom::{DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view;
use zgui::view::AnyView;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 80px 40px; align-items: flex-start }
                     .pair { flex-direction: row; gap: 40px; align-items: flex-start }
                     .tight { font-size: 40px; line-height: 4px }
                     .loose { font-size: 40px; line-height: 80px }";

/// Long enough for the frames a change asks for to run.
const FRAMES: Duration = Duration::from_millis(50);

/// What the line says, written from outside the page.
type Label = RwSignal<&'static str, zgui::reactive::LocalStorage>;

/// The page around the line, where its glyphs can reach.
fn around(stage: &Stage) -> Vec<(u8, u8, u8)> {
    stage.composed_colours_in(Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(600.0), DevicePx(200.0)),
    ))
}

#[test]
fn a_tight_line_that_changes_is_repainted_where_its_glyphs_reach() {
    crate::device::use_real_damage();
    let handed: Rc<RefCell<Option<Label>>> = Rc::new(RefCell::new(None));
    let held = Rc::clone(&handed);
    let Some(mut stage) = Stage::open(SHEET, move || {
        let label = RwSignal::new_local("gjpqy gjpqy");
        *held.borrow_mut() = Some(label);
        AnyView::new(view! {
            column(class = "page") {
                text(class = "tight") {{move || label.get()}}
            }
        })
    }) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    stage.wait_quietly(FRAMES);
    let label = handed.borrow().expect("the page was built");

    for next in ["ACE ACE", "gjpqy gjpqy", "Åçé gjy"] {
        label.set(next);
        stage.wait_quietly(FRAMES);
        let partial = around(&stage);
        stage.repaint();
        stage.wait_quietly(FRAMES);
        let whole = around(&stage);
        let differing = partial
            .iter()
            .zip(&whole)
            .filter(|(left, right)| left != right)
            .count();
        assert_eq!(
            differing, 0,
            "after the line became {next:?}, {differing} pixels differ from a repaint of the whole \
             window"
        );
    }
}

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
