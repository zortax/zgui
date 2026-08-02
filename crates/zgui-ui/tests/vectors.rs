//! Where a batch of drawings lands when the page around them is not flat.
//!
//! A rasterisation pass covers the rectangle enclosing every drawing in it and is composited by a
//! *single* draw, inserted where the pass's last item falls in the painting order. Everything that
//! decides where one pass ends and the next begins therefore decides whether a drawing is on the
//! screen at all — and the cases that get it wrong are the ones a fixture with two icons side by
//! side never reaches: drawings far apart, and drawings each inside something that isolates itself.

mod device;

use zgui::view;
use zgui::{component, prelude::*};
use zgui_ui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::CHECK;

use crate::device::frame::Frame;

/// How many drawings the fixtures spread out.
const RUNGS: usize = 22;

/// The sheet: a flat page, a column of panels, and one indent rule per panel.
///
/// The indents are written into the sheet rather than onto the elements because that is where a
/// length reliably reaches layout from, and the distance between the drawings is the whole variable
/// this fixture turns.
fn sheet(step: f32) -> String {
    let mut css = String::from(
        ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
         .page { padding: 8px }
         .spread { flex-direction: column; align-items: flex-start }
         .rung { align-items: center; background-color: #f0f0f0; padding: 6px }",
    );
    for index in 0..RUNGS {
        css.push_str(&format!(
            ".i{index} {{ margin-left: {}px }}",
            index as f32 * step
        ));
    }
    css
}

/// A column of icons, each one further right than the last, every one on a panel of its own.
///
/// The panels are what makes this the shape a real page produces rather than the shape a two-icon
/// fixture produces: each one is an opaque box emitted *between* two drawings, which is what the
/// coalescing policy has to reason about when it decides where one pass ends.
#[component]
fn Diagonal() -> impl IntoView {
    let rungs = (0..RUNGS).map(|index| {
        let class = format!("rung i{index}");
        view! {
            row(class = class) {
                Icon(icon = CHECK, size = IconSize::Md)
            }
        }
    });
    view! {
        column(class = "page") {
            column(class = "spread") {{rungs.collect::<Vec<_>>()}}
        }
    }
}

/// A row of controls, each inside something that isolates it, all of them showing their mark.
///
/// The wrapper's opacity is what makes each one a target of its own — the same thing a fade, a
/// filter or a blend mode does, and the shape of every control that animates its mark in and out.
/// Nothing is drawn between them and none of them overlaps another, so every rule about *what is
/// painted between two drawings* says the marks may share one rasterisation pass. They may not:
/// one composite cannot be recorded in six targets at once.
#[component]
fn Isolated() -> impl IntoView {
    let ticked = zgui::reactive::RwSignal::new_local(Checked::Yes);
    view! {
        column(class = "page") {
            row(class = "spread") {
                box(class = "iso") {Checkbox(checked = ticked, a11y:label = "One")}
                box(class = "iso") {Checkbox(checked = ticked, a11y:label = "Two")}
                box(class = "iso") {Checkbox(checked = ticked, a11y:label = "Three")}
                box(class = "iso") {Checkbox(checked = ticked, a11y:label = "Four")}
                box(class = "iso") {Checkbox(checked = ticked, a11y:label = "Five")}
                box(class = "iso") {Checkbox(checked = ticked, a11y:label = "Six")}
            }
        }
    }
}

/// Draws `view` on a surface of the given extent and hands back the last frame that held a drawing.
fn draw<V: IntoView + 'static>(
    width: f32,
    height: f32,
    css: String,
    view: impl FnMut() -> V + 'static,
) -> Option<Frame> {
    if !device::available() {
        return None;
    }
    let _guard = device::device_lock();
    let log: device::Log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    zgui::app()
        .with_title("vectors")
        .with_size(width, height)
        .with_stylesheet(css)
        .with_renderer(Box::new(device::factory(&log)))
        .run_on(
            move |handler| {
                let mut harness = zgui_platform_headless::Harness::new(handler);
                harness.deliver_to_first(zgui::platform::SurfaceEvent::Resized(
                    zgui::geom::Size::new(
                        zgui::geom::DevicePx(width),
                        zgui::geom::DevicePx(height),
                    ),
                ));
                harness.settle(96);
                harness.shut_down();
                Ok(())
            },
            view,
        )
        .expect("the document ran");
    let frames = core::mem::take(&mut *log.lock().unwrap_or_else(|held| held.into_inner()));
    frames
        .into_iter()
        .rev()
        .find(|frame| !frame.drawings.is_empty())
}

/// Draws the diagonal with `step` between its icons, on a surface big enough to hold every one.
fn diagonal(step: f32) -> Option<Frame> {
    // Room for every rung: the indents accumulate across the column, so the last one sits at the
    // sum of all of them. A surface too small for the last drawing puts it off the page, where it
    // is not painted — and a fixture that mistook that for a rasteriser writing nothing would be
    // measuring its own arithmetic.
    let width = step * RUNGS as f32 + 256.0;
    let height = RUNGS as f32 * 48.0 + 256.0;
    draw(width, height, sheet(step), || view! { Diagonal() })
}

/// How much of its own rectangle each drawing of `frame` marked.
fn inked(frame: &Frame) -> Vec<f32> {
    frame
        .drawings
        .iter()
        .map(|drawing| device::ink::fraction(&frame.pixels, drawing.ink))
        .collect()
}

#[test]
fn icons_spread_across_a_page_all_draw() {
    let Some(frame) = diagonal(400.0) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    assert_eq!(
        frame.drawings.len(),
        RUNGS,
        "every icon reached the display list"
    );
    let ink = inked(&frame);
    assert!(
        ink.iter().all(|fraction| *fraction > 0.0),
        "a pass covering most of the page left some of its drawings unmarked: {ink:?}"
    );
}

#[test]
fn icons_beside_each_other_all_draw() {
    let Some(frame) = diagonal(24.0) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let ink = inked(&frame);
    assert!(
        ink.iter().all(|fraction| *fraction > 0.0),
        "some of the icons left their own rectangle flat: {ink:?}"
    );
}

#[test]
fn every_mark_in_a_row_of_isolated_controls_is_drawn() {
    let Some(frame) = draw(
        640.0,
        160.0,
        ":root { background-color: #ffffff; font-family: sans-serif }
         .page { padding: 16px }
         .spread { gap: 16px; align-items: center }
         .iso { opacity: 0.9 }"
            .to_owned(),
        || view! { Isolated() },
    ) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    assert!(
        frame.drawings.len() >= 6,
        "six ticked checkboxes produced only {} drawings",
        frame.drawings.len()
    );
    assert!(
        frame.pass_regions.len() >= 6,
        "six drawings in six isolated controls were planned as {} passes; one composite cannot \
         be recorded in six targets at once",
        frame.pass_regions.len()
    );
    // A checkbox carries both of its marks and shows one: a ticked box draws its tick and not the
    // dash it would show if it were part-way. So half the drawings are *meant* to be invisible, and
    // the number that must be on the screen is one per control rather than one per drawing.
    let ink = inked(&frame);
    let drawn = ink.iter().filter(|fraction| **fraction > 0.0).count();
    assert_eq!(
        drawn, 6,
        "six ticked checkboxes have to show six ticks; the ink each drawing left was {ink:?}"
    );
}
