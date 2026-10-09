//! A box that changes size leaves the picture a complete redraw would.
//!
//! A box whose painting is a fill, a border and fixed corners changes only near the edges that
//! moved, so a resize repaints strips along those edges and keeps the rest of what was composed.
//! A box whose painting depends on its size elsewhere — a percentage corner, a shadow, a gradient,
//! a scroll container's bars — is repainted whole. Each partial frame is compared with a complete
//! redraw of the same document, after a panel's edge is dragged and after the window is resized
//! inside its composed target and past it.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::view;
use zgui::view::AnyView;

use crate::painted::stage::Stage;

const SHEET: &str = ":root { background-color: rgb(18, 52, 104) }
    .shell { width: 100%; height: 100%; flex-direction: column; padding: 6px; gap: 6px }
    .top { flex-direction: row; gap: 6px; padding: 8px; background-color: rgb(40, 44, 52);
           border: 2px solid rgb(200, 160, 60); border-radius: 9px; overflow: hidden }
    .bottom { flex: 1 1 auto; flex-direction: column; padding: 8px; gap: 4px;
              background-color: rgb(30, 90, 60); border-top: 3px solid rgb(240, 240, 240) }
    .pill { width: 30%; height: 40px; background-color: rgb(160, 40, 80); border-radius: 50% }
    .shadowed { width: 120px; height: 40px; background-color: rgb(90, 90, 160);
                box-shadow: 0 4px 12px rgba(0, 0, 0, 0.6) }
    .gradient { width: 25%; height: 40px;
                background-image: linear-gradient(90deg, rgb(255, 0, 0), rgb(0, 0, 255)) }
    .scroller { flex: 1 1 auto; overflow: auto; background-color: rgb(70, 70, 70) }
    .tall { height: 600px; background-color: rgb(110, 110, 30) }
    .chip { width: 60px; height: 30px; background-color: rgb(220, 120, 40); border-radius: 4px }
    .line { color: rgb(250, 250, 250) }";

/// Two panels whose own painting is a fill, a border and fixed corners, with nothing inside them
/// that repaints itself whole: whatever the resize owes, the panels' own strips have to cover.
fn plain(top: RwSignal<f32>) -> impl Fn() -> AnyView {
    move || {
        AnyView::new(view! {
            column(class = "shell") {
                row(class = "top", style:height = move || Some(format!("{}px", top.get()))) {
                    box(class = "chip") {}
                }
                column(class = "bottom") {
                    text(class = "line") {"The bottom panel keeps its words where they were."}
                }
            }
        })
    }
}

/// The same panels holding boxes whose painting depends on their size: a percentage corner, a
/// shadow, a gradient and a scroll container. Those are repainted whole.
fn sized(top: RwSignal<f32>) -> impl Fn() -> AnyView {
    move || {
        AnyView::new(view! {
            column(class = "shell") {
                row(class = "top", style:height = move || Some(format!("{}px", top.get()))) {
                    box(class = "pill") {}
                    box(class = "shadowed") {}
                    box(class = "gradient") {}
                }
                column(class = "bottom") {
                    text(class = "line") {"The bottom panel keeps its words where they were."}
                    column(class = "scroller") {box(class = "tall") {}}
                }
            }
        })
    }
}

fn picture(stage: &Stage, width: f32, height: f32) -> Vec<(u8, u8, u8)> {
    stage.composed_colours_in(Rect::<DevicePx, Device>::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(width), DevicePx(height)),
    ))
}

fn differing(a: &[(u8, u8, u8)], b: &[(u8, u8, u8)]) -> usize {
    a.iter()
        .zip(b)
        .filter(|(x, y)| {
            let d = |p: u8, q: u8| (i32::from(p) - i32::from(q)).abs();
            d(x.0, y.0) + d(x.1, y.1) + d(x.2, y.2) > 24
        })
        .count()
}

/// Compares what the partial frames left with a complete redraw, and names the step if they differ.
fn compare(stage: &mut Stage, width: f32, height: f32, step: &str, failures: &mut Vec<String>) {
    let partial = picture(stage, width, height);
    stage.repaint();
    let full = picture(stage, width, height);
    let wrong = differing(&partial, &full);
    if wrong > 0 {
        stage.capture_composed(&format!("resized-{step}"));
        failures.push(format!("{step}: {wrong} pixels"));
    }
}

/// Drags the top panel's lower edge down and back, comparing every step with a complete redraw.
fn drag(scene: fn(RwSignal<f32>) -> Box<dyn Fn() -> AnyView>) -> Vec<String> {
    crate::device::use_real_damage();
    let top = RwSignal::new(160.0f32);
    let Some(mut stage) = Stage::open(SHEET, scene(top)) else {
        eprintln!("skipped: no usable graphics device");
        return Vec::new();
    };
    stage.wait_quietly(std::time::Duration::from_millis(34));
    stage.repaint();
    let mut failures = Vec::new();
    for step in 1..16 {
        let height = 160.0
            + if step < 8 {
                step as f32 * 7.3
            } else {
                (16 - step) as f32 * 7.3
            };
        top.set(height);
        stage.wait_quietly(std::time::Duration::from_millis(34));
        compare(
            &mut stage,
            900.0,
            600.0,
            &format!("drag-{step}"),
            &mut failures,
        );
    }
    failures
}

/// Resizes the window inside its composed target and past it, comparing every size with a
/// complete redraw.
fn resize(scene: fn(RwSignal<f32>) -> Box<dyn Fn() -> AnyView>) -> Vec<String> {
    crate::device::use_real_damage();
    let top = RwSignal::new(160.0f32);
    let Some(mut stage) = Stage::open(SHEET, scene(top)) else {
        eprintln!("skipped: no usable graphics device");
        return Vec::new();
    };
    stage.wait_quietly(std::time::Duration::from_millis(34));
    stage.repaint();
    let mut failures = Vec::new();
    // Shrinking and growing inside one allocation, then past it, then back.
    let sizes = [
        (890.0, 596.0),
        (871.0, 583.0),
        (885.0, 590.0),
        (800.0, 520.0),
        (860.0, 575.0),
        (500.0, 400.0),
        (880.0, 590.0),
    ];
    for (index, (width, height)) in sizes.into_iter().enumerate() {
        stage.resize(width, height);
        stage.wait_quietly(std::time::Duration::from_millis(34));
        compare(
            &mut stage,
            width,
            height,
            &format!("size-{index}-{width}x{height}"),
            &mut failures,
        );
    }
    failures
}

fn boxed_plain(top: RwSignal<f32>) -> Box<dyn Fn() -> AnyView> {
    Box::new(plain(top))
}

fn boxed_sized(top: RwSignal<f32>) -> Box<dyn Fn() -> AnyView> {
    Box::new(sized(top))
}

#[test]
fn a_plain_panel_dragged_taller_and_shorter_leaves_no_trail() {
    let failures = drag(boxed_plain);
    assert!(
        failures.is_empty(),
        "a dragged edge left pixels behind: {failures:#?}"
    );
}

#[test]
fn a_panel_of_sized_paintings_dragged_taller_and_shorter_leaves_no_trail() {
    let failures = drag(boxed_sized);
    assert!(
        failures.is_empty(),
        "a dragged edge left pixels behind: {failures:#?}"
    );
}

#[test]
fn a_plain_window_resized_inside_and_past_its_target_leaves_no_trail() {
    let failures = resize(boxed_plain);
    assert!(
        failures.is_empty(),
        "a resized window left pixels behind: {failures:#?}"
    );
}

#[test]
fn a_window_of_sized_paintings_resized_leaves_no_trail() {
    let failures = resize(boxed_sized);
    assert!(
        failures.is_empty(),
        "a resized window left pixels behind: {failures:#?}"
    );
}
